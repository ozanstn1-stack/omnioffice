//! AcroForm form fields and page-object editing.
//!
//! This module does two related jobs, both of them on real PDF objects - no
//! simulation, no "pretend" state kept outside the file:
//!
//! 1. **Form fields (AcroForm).** `list_fields` walks the field tree properly,
//!    honoring the inheritable attributes (`/FT`, `/Ff`, `/V`, `/DV`, `/DA`)
//!    down a `/Parent` chain and through intermediate `/Kids` nodes, so a field
//!    whose `/FT` lives on an ancestor is still reported correctly.
//!    `apply_field_values` writes `/V` (leaving `/DV` untouched) and rebuilds
//!    the widget appearance streams with a Base14 Helvetica font, and
//!    `validate_fields` performs the checks that are provable from the file
//!    contents alone. PDF JavaScript is **never** executed: a field whose
//!    format/validation lives in an `/AA` action is reported as "scripted
//!    format not checked" instead of guessing.
//!
//!    Flattening after a fill is the existing `pdfcore::flatten` module; this
//!    module deliberately does not duplicate it.
//!
//! 2. **Page objects.** `list_page_objects` reports every page annotation and
//!    widget, plus every image XObject a page actually draws (with its
//!    placement matrix). `transform_objects` moves/resizes/rotates/deletes
//!    annotations by rewriting their `/Rect` (and appearance matrix for
//!    rotation), removes deleted widgets from the AcroForm field tree, and
//!    edits image placements by rewriting *only* the six numbers of the `cm`
//!    matrix that feeds the matching `/Name Do` in the content stream - the
//!    rest of the decoded stream stays byte-identical.
//!
//!    Honest limitations, on purpose:
//!    * Text and vector objects *inside* content streams are **not** listed or
//!      edited. Doing that reliably would mean implementing the full graphics
//!      state machine (text matrices, fonts, clipping, nested form XObjects)
//!      and then re-encoding the stream, which is exactly the "rewrite
//!      arbitrary content safely" problem. Instead, the content-stream editor
//!      works on the one construct that can be located unambiguously from the
//!      bytes: the `cm` + `/Name Do` sequence of an image placement. Anything
//!      else in a stream is reported as untouched rather than guessed at.
//!    * Form values that use characters outside WinAnsi cannot be drawn by a
//!      Base14 Helvetica appearance; they are transliterated to `?` in the
//!      generated appearance while `/V` keeps the full Unicode string and the
//!      form is marked `/NeedAppearances`, so a viewer with font support can
//!      re-render it.
//!    * `validate_fields` cannot check JavaScript-format masks. It checks
//!      required/read-only, `/MaxLen`, choice membership, and - only when the
//!      field name or tooltip suggests it - common date/number shapes. Those
//!      heuristic checks are reported with severity `warning`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::docutil::{self, Matrix};
use crate::error::{PdfError, PdfResult};

// ---------------------------------------------------------------------------
// Field flags (PDF 1.7, table 226 / 228 / 230)
// ---------------------------------------------------------------------------

const FLAG_READ_ONLY: i64 = 1; // bit 1 (all fields)
const FLAG_REQUIRED: i64 = 2; // bit 2 (all fields)
const FLAG_MULTILINE: i64 = 1 << 12; // Tx bit 13
const FLAG_PASSWORD: i64 = 1 << 13; // Tx bit 14
const FLAG_RADIO: i64 = 1 << 15; // Btn bit 16
const FLAG_PUSHBUTTON: i64 = 1 << 16; // Btn bit 17
const FLAG_COMBO: i64 = 1 << 17; // Ch bit 18
const FLAG_EDIT: i64 = 1 << 18; // Ch bit 19
const FLAG_MULTI_SELECT: i64 = 1 << 21; // Ch bit 22
const FLAG_COMB: i64 = 1 << 24; // Tx bit 25

// ---------------------------------------------------------------------------
// Public types: form fields
// ---------------------------------------------------------------------------

/// One entry of a choice field's `/Opt` array (and of the synthesized options
/// for radio button groups).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldOption {
    /// The value written to `/V` when this option is chosen.
    pub value: String,
    /// The human readable label (the second entry of an `/Opt` pair).
    pub label: String,
}

/// A form field as found in the document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FormFieldInfo {
    /// Fully qualified name (`/T` joined with the parent chain, dot separated).
    pub name: String,
    /// `text` | `checkbox` | `radio` | `pushbutton` | `choice` | `signature` | `unknown`.
    pub field_type: String,
    /// Raw `/Ff` value after inheritance.
    pub flags: i64,
    pub required: bool,
    pub read_only: bool,
    /// Current value (`/V`), as a string. Arrays (multi select) are joined in
    /// `values`.
    pub value: String,
    /// All current values (multi-select choice fields can have several).
    pub values: Vec<String>,
    pub default_value: String,
    /// `/TU` tooltip, when present.
    pub tooltip: Option<String>,
    /// `/MaxLen` for text fields.
    pub max_length: Option<i64>,
    pub multiline: bool,
    pub password: bool,
    pub comb: bool,
    /// Choice field is a dropdown (`/Ff` combo bit) rather than a list box.
    pub combo: bool,
    /// Choice field is editable (`/Ff` edit bit).
    pub editable: bool,
    pub multi_select: bool,
    /// Declared options. Radio groups list one entry per widget state.
    pub options: Vec<FieldOption>,
    /// Page of the first widget, when it could be resolved.
    pub page: Option<u32>,
    /// `/Rect` of the first widget (page space, bottom-left origin).
    pub rect: Option<[f64; 4]>,
    /// Position in the page tab order when the page declares a `/Tabs` number
    /// tree; `None` means annotation order applies.
    pub tab_order: Option<u32>,
    pub widget_count: u32,
    /// The field carries JavaScript actions (`/AA` or `/A`); validation cannot
    /// execute them and says so in the field issue list.
    pub has_script: bool,
}

/// A value requested for one field.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldValue {
    pub name: String,
    /// Single value: text, choice export, checkbox/radio state.
    pub value: String,
    /// Multi-select choice values. When non-empty this wins over `value`.
    pub values: Vec<String>,
}

/// What `apply_field_values` actually changed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FillReport {
    pub filled: u32,
    /// Fields that were requested but deliberately not written (read-only,
    /// push buttons, signatures, unsupported types).
    pub skipped: Vec<String>,
    /// Values written with a caveat (over `/MaxLen`, unknown checkbox state).
    pub warnings: Vec<String>,
}

/// One validation finding. Nothing here is produced by executing scripts.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldIssue {
    pub field: String,
    /// Stable machine code: `required`, `max_length`, `option_not_in_list`,
    /// `unknown_field`, `read_only`, `not_fillable`, `invalid_date`,
    /// `invalid_number`, `multi_select_not_allowed`, `scripted_format`.
    pub code: String,
    /// `error` for provable violations, `warning` for heuristic checks.
    pub severity: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Internal field model
// ---------------------------------------------------------------------------

/// Snapshot of a terminal field node and its widgets.
#[derive(Debug, Clone)]
struct FieldNode {
    id: ObjectId,
    dict: Dictionary,
    full_name: String,
    ft: Vec<u8>,
    ff: i64,
    /// Inherited `/V` (the field value; may live on an ancestor /Kids node).
    value: Option<Object>,
    default_value: Option<Object>,
    max_len: Option<i64>,
    da: Option<String>,
    options: Vec<FieldOption>,
    /// Widget annotations: the node itself when it is a merged field+widget,
    /// plus any `/Kids` that are widgets.
    widgets: Vec<(ObjectId, Dictionary)>,
}

#[derive(Debug, Clone, Default)]
struct Inherited {
    ft: Option<Vec<u8>>,
    ff: i64,
    v: Option<Object>,
    dv: Option<Object>,
    da: Option<String>,
}

// ---------------------------------------------------------------------------
// Generic object helpers (kept small and panic-free)
// ---------------------------------------------------------------------------

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

fn dict_name(dict: &Dictionary, key: &[u8]) -> Option<Vec<u8>> {
    dict.get(key).ok().and_then(|value| value.as_name().ok()).map(|name| name.to_vec())
}

fn dict_i64(dict: &Dictionary, key: &[u8]) -> Option<i64> {
    dict.get(key).ok().and_then(|value| value.as_i64().ok())
}

/// Reads a text-ish dictionary entry, following one reference.
fn dict_text(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<String> {
    let value = dict.get(key).ok()?;
    match value {
        Object::Reference(id) => doc.get_object(*id).ok().and_then(docutil::pdf_text_value),
        other => docutil::pdf_text_value(other),
    }
}

/// Reads `/Rect` from an annotation dictionary, following one reference.
fn rect_of(doc: &Document, dict: &Dictionary) -> Option<[f64; 4]> {
    let value = dict.get(b"Rect").ok()?;
    let resolved = match value {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let items = resolved.as_array().ok()?;
    if items.len() < 4 {
        return None;
    }
    let mut out = [0.0f64; 4];
    for (index, item) in items.iter().take(4).enumerate() {
        out[index] = docutil::object_to_f64(item)?;
    }
    Some(out)
}

fn rect_normalize(rect: [f64; 4]) -> [f64; 4] {
    [
        rect[0].min(rect[2]),
        rect[1].min(rect[3]),
        rect[0].max(rect[2]),
        rect[1].max(rect[3]),
    ]
}

fn rect_dims(rect: [f64; 4]) -> (f64, f64) {
    let r = rect_normalize(rect);
    ((r[2] - r[0]).abs().max(1.0), (r[3] - r[1]).abs().max(1.0))
}

fn rect_to_object(rect: [f64; 4]) -> Object {
    Object::Array(vec![
        Object::Real(rect[0] as f32),
        Object::Real(rect[1] as f32),
        Object::Real(rect[2] as f32),
        Object::Real(rect[3] as f32),
    ])
}

fn matrix_from_object(value: &Object) -> Option<Matrix> {
    let items = value.as_array().ok()?;
    if items.len() < 6 {
        return None;
    }
    let mut values = [0.0f64; 6];
    for (index, item) in items.iter().take(6).enumerate() {
        values[index] = docutil::object_to_f64(item).unwrap_or(if index == 0 || index == 3 { 1.0 } else { 0.0 });
    }
    Some(Matrix(values))
}

/// Numeric array of any length (used for appearance `/BBox`, which has 4).
fn numbers_from_object(value: &Object) -> Option<Vec<f64>> {
    let items = value.as_array().ok()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(docutil::object_to_f64(item)?);
    }
    Some(out)
}

fn matrix_to_object(matrix: Matrix) -> Object {
    Object::Array(matrix.0.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// Applies a closure to an indirect object's dictionary.
fn with_object_dict<R>(doc: &mut Document, id: ObjectId, f: impl FnOnce(&mut Dictionary) -> R) -> PdfResult<R> {
    let dict = doc
        .get_object_mut(id)
        .map_err(PdfError::from)?
        .as_dict_mut()
        .map_err(PdfError::from)?;
    Ok(f(dict))
}

fn set_dict_entry(doc: &mut Document, id: ObjectId, key: &[u8], value: Object) -> PdfResult<()> {
    with_object_dict(doc, id, |dict| dict.set(key.to_vec(), value))
}

/// All page ids, keyed by 1-based page number.
fn page_map(doc: &Document) -> BTreeMap<u32, ObjectId> {
    doc.get_pages()
}

/// Resolves the annotations array of a page, whether inline or indirect.
fn page_annots(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    let page = match doc.get_dictionary(page_id) {
        Ok(page) => page,
        Err(_) => return Vec::new(),
    };
    resolve_array(doc, page.get(b"Annots").ok()).unwrap_or_default()
}

/// annotation object id -> page number (first page that lists it).
fn annotation_pages(doc: &Document, pages: &BTreeMap<u32, ObjectId>) -> HashMap<ObjectId, u32> {
    let mut out = HashMap::new();
    for (number, page_id) in pages {
        for entry in page_annots(doc, *page_id) {
            if let Ok(id) = entry.as_reference() {
                out.entry(id).or_insert(*number);
            }
        }
    }
    out
}

/// Flattens a page's `/Tabs` number tree into annotation -> tab index.
///
/// The PDF spec lets a page order its annotations for keyboard navigation in
/// a number tree; when that exists its order is authoritative. When it does
/// not (or when it is a name such as `/A`), annotation array order is used and
/// this returns an empty map.
fn tab_order_map(doc: &Document, page_id: ObjectId) -> HashMap<ObjectId, u32> {
    let page = match doc.get_dictionary(page_id) {
        Ok(page) => page,
        Err(_) => return HashMap::new(),
    };
    let tree = match resolve_dict(doc, page.get(b"Tabs").ok()) {
        Some(tree) => tree,
        None => return HashMap::new(),
    };
    let mut out = HashMap::new();
    let mut counter = 0u32;
    collect_tab_tree(doc, &tree, &mut out, &mut counter, 0);
    out
}

fn collect_tab_tree(
    doc: &Document,
    tree: &Dictionary,
    out: &mut HashMap<ObjectId, u32>,
    counter: &mut u32,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    if let Some(nums) = resolve_array(doc, tree.get(b"Nums").ok()) {
        let mut index = 0usize;
        while index + 1 < nums.len() {
            if let Ok(id) = nums[index + 1].as_reference() {
                out.entry(id).or_insert(*counter);
            }
            *counter = counter.saturating_add(1);
            index += 2;
        }
    }
    if let Some(kids) = resolve_array(doc, tree.get(b"Kids").ok()) {
        for kid in kids {
            if let Some(dict) = resolve_dict(doc, Some(&kid)) {
                collect_tab_tree(doc, &dict, out, counter, depth + 1);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Field tree walking
// ---------------------------------------------------------------------------

fn collect_field_nodes(doc: &Document) -> Vec<FieldNode> {
    let mut out = Vec::new();
    let catalog = match doc.catalog() {
        Ok(catalog) => catalog,
        Err(_) => return out,
    };
    let acro = match resolve_dict(doc, catalog.get(b"AcroForm").ok()) {
        Some(acro) => acro,
        None => return out,
    };
    let fields = resolve_array(doc, acro.get(b"Fields").ok()).unwrap_or_default();
    for field in fields {
        if let Ok(id) = field.as_reference() {
            walk_field(doc, id, "", &Inherited::default(), &mut out, 0);
        }
    }
    out
}

fn walk_field(
    doc: &Document,
    id: ObjectId,
    prefix: &str,
    inherited: &Inherited,
    out: &mut Vec<FieldNode>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let dict = match doc.get_dictionary(id) {
        Ok(dict) => dict.clone(),
        Err(_) => return,
    };
    let ft = dict_name(&dict, b"FT").or_else(|| inherited.ft.clone());
    let ff = dict_i64(&dict, b"Ff").unwrap_or(inherited.ff);
    // /V, /DV, /DA and /Ff are inheritable: a radio group commonly keeps its
    // /V on the parent while the widget kids hold no value at all.
    let v = dict.get(b"V").ok().cloned().or_else(|| inherited.v.clone());
    let dv = dict.get(b"DV").ok().cloned().or_else(|| inherited.dv.clone());
    let da = dict_text(doc, &dict, b"DA").or_else(|| inherited.da.clone());
    let partial = dict_text(doc, &dict, b"T").unwrap_or_default();
    let full_name = if partial.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        partial
    } else {
        format!("{prefix}.{partial}")
    };
    let next = Inherited {
        ft: ft.clone(),
        ff,
        v: v.clone(),
        dv: dv.clone(),
        da: da.clone(),
    };

    let mut widgets: Vec<(ObjectId, Dictionary)> = Vec::new();
    let mut field_kids: Vec<ObjectId> = Vec::new();
    if let Some(kids) = resolve_array(doc, dict.get(b"Kids").ok()) {
        for kid in kids {
            let kid_id = match kid.as_reference() {
                Ok(kid_id) => kid_id,
                Err(_) => continue,
            };
            let kid_dict = match doc.get_dictionary(kid_id) {
                Ok(kid_dict) => kid_dict.clone(),
                Err(_) => continue,
            };
            let is_widget = dict_name(&kid_dict, b"Subtype").as_deref() == Some(b"Widget".as_slice());
            // A widget kid normally has no /T and no /FT of its own; a kid
            // that has either is a merged field in its own right and must be
            // walked as a child field.
            if is_widget && kid_dict.get(b"T").is_err() && kid_dict.get(b"FT").is_err() {
                widgets.push((kid_id, kid_dict));
            } else {
                field_kids.push(kid_id);
            }
        }
    }
    if dict_name(&dict, b"Subtype").as_deref() == Some(b"Widget".as_slice()) {
        widgets.insert(0, (id, dict.clone()));
    }

    if field_kids.is_empty() {
        // Terminal field. Fields with no /FT at all are still reported when
        // they carry a name, as "unknown" (some producers split dictionaries
        // oddly); the report is honest about it.
        if ft.is_some() || !full_name.is_empty() || !widgets.is_empty() {
            let options = if ft.as_deref() == Some(b"Ch".as_slice()) || ft.as_deref() == Some(b"Btn".as_slice()) {
                choice_options(doc, &dict)
            } else {
                Vec::new()
            };
            let max_len = dict_i64(&dict, b"MaxLen");
            out.push(FieldNode {
                id,
                dict,
                full_name,
                ft: ft.unwrap_or_default(),
                ff,
                value: v,
                default_value: dv,
                max_len,
                da,
                options,
                widgets,
            });
        }
    } else {
        for kid_id in field_kids {
            walk_field(doc, kid_id, &full_name, &next, out, depth + 1);
        }
    }
}

/// Parses `/Opt`: a string is both value and label; an array is
/// `[export, label]` (the label may be missing).
fn choice_options(doc: &Document, dict: &Dictionary) -> Vec<FieldOption> {
    let items = match resolve_array(doc, dict.get(b"Opt").ok()) {
        Some(items) => items,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in items {
        match &item {
            Object::Array(pair) => {
                let value = pair.first().and_then(docutil::pdf_text_value).unwrap_or_default();
                let label = pair
                    .get(1)
                    .and_then(docutil::pdf_text_value)
                    .unwrap_or_else(|| value.clone());
                out.push(FieldOption { value, label });
            }
            other => {
                if let Some(text) = docutil::pdf_text_value(other) {
                    out.push(FieldOption { value: text.clone(), label: text });
                }
            }
        }
    }
    out
}

/// The "on" appearance states a widget declares (`/AP /N` dictionary keys
/// other than `Off`). These are the export values of checkbox/radio widgets.
fn widget_on_states(doc: &Document, widget: &Dictionary) -> Vec<String> {
    let ap = resolve_dict(doc, widget.get(b"AP").ok());
    let normal = ap.and_then(|ap| ap.get(b"N").ok().cloned());
    let states = match normal {
        Some(Object::Dictionary(dict)) => Some(dict),
        Some(Object::Reference(id)) => match doc.get_object(id) {
            Ok(Object::Dictionary(dict)) => Some(dict.clone()),
            _ => None,
        },
        _ => None,
    };
    let mut out = Vec::new();
    if let Some(states) = states {
        for (key, _) in states.iter() {
            let name = String::from_utf8_lossy(key).to_string();
            if name != "Off" && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// A field value object (possibly an array) rendered as a list of strings.
fn value_strings(doc: &Document, value: &Object) -> Vec<String> {
    match value {
        Object::Array(items) => items.iter().flat_map(|item| value_strings(doc, item)).collect(),
        Object::Reference(id) => doc
            .get_object(*id)
            .ok()
            .map(|resolved| value_strings(doc, resolved))
            .unwrap_or_default(),
        other => docutil::pdf_text_value(other).map(|text| vec![text]).unwrap_or_default(),
    }
}

/// The current value of a node (inherited /V).
fn node_values(doc: &Document, node: &FieldNode) -> Vec<String> {
    node.value
        .as_ref()
        .map(|value| value_strings(doc, value))
        .unwrap_or_default()
}

fn node_field_type(node: &FieldNode) -> &'static str {
    match node.ft.as_slice() {
        b"Tx" => "text",
        b"Btn" => {
            if node.ff & FLAG_PUSHBUTTON != 0 {
                "pushbutton"
            } else if node.ff & FLAG_RADIO != 0 {
                "radio"
            } else {
                "checkbox"
            }
        }
        b"Ch" => "choice",
        b"Sig" => "signature",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// list_fields
// ---------------------------------------------------------------------------

/// Walks the AcroForm field tree and returns one entry per terminal field.
pub fn list_fields(doc: &Document) -> Vec<FormFieldInfo> {
    let nodes = collect_field_nodes(doc);
    let pages = page_map(doc);
    let annot_pages = annotation_pages(doc, &pages);
    let mut tab_orders: HashMap<u32, HashMap<ObjectId, u32>> = HashMap::new();
    for (number, page_id) in &pages {
        tab_orders.insert(*number, tab_order_map(doc, *page_id));
    }

    let mut out = Vec::new();
    for node in &nodes {
        let field_type = node_field_type(node);
        let values = node_values(doc, node);
        let options = match field_type {
            "choice" => node.options.clone(),
            "radio" => radio_options(doc, node),
            "checkbox" => checkbox_states(doc, node)
                .into_iter()
                .map(|state| FieldOption { value: state.clone(), label: state })
                .collect(),
            _ => Vec::new(),
        };
        // First widget that resolves to a page wins, for the UI overlay.
        let mut page = None;
        let mut rect = None;
        let mut tab_order = None;
        for (widget_id, widget) in &node.widgets {
            let widget_page = widget
                .get(b"P")
                .ok()
                .and_then(|value| value.as_reference().ok())
                .and_then(|page_id| pages.iter().find(|(_, id)| **id == page_id).map(|(number, _)| *number))
                .or_else(|| annot_pages.get(widget_id).copied());
            if page.is_none() && widget_page.is_some() {
                page = widget_page;
                rect = rect_of(doc, widget);
            }
            if tab_order.is_none() {
                if let Some(widget_page) = widget_page {
                    if let Some(order) = tab_orders.get(&widget_page).and_then(|map| map.get(widget_id)) {
                        tab_order = Some(*order);
                    }
                }
            }
        }
        let has_script = node.dict.get(b"AA").is_ok() || node.dict.get(b"A").is_ok();
        let default_value = node
            .default_value
            .as_ref()
            .map(|value| value_strings(doc, value).join(", "))
            .unwrap_or_default();
        out.push(FormFieldInfo {
            name: node.full_name.clone(),
            field_type: field_type.to_string(),
            flags: node.ff,
            required: node.ff & FLAG_REQUIRED != 0,
            read_only: node.ff & FLAG_READ_ONLY != 0,
            value: values.first().cloned().unwrap_or_default(),
            values,
            default_value,
            tooltip: dict_text(doc, &node.dict, b"TU"),
            max_length: node.max_len,
            multiline: node.ff & FLAG_MULTILINE != 0,
            password: node.ff & FLAG_PASSWORD != 0,
            comb: node.ff & FLAG_COMB != 0,
            combo: node.ff & FLAG_COMBO != 0,
            editable: node.ff & FLAG_EDIT != 0,
            multi_select: node.ff & FLAG_MULTI_SELECT != 0,
            options,
            page,
            rect,
            tab_order,
            widget_count: node.widgets.len() as u32,
            has_script,
        });
    }
    out
}

/// Radio options: one entry per widget "on" state; labels come from the
/// field's `/Opt` array when the producer supplied one (matched by widget
/// order), otherwise the state name is both value and label.
fn radio_options(doc: &Document, node: &FieldNode) -> Vec<FieldOption> {
    let labels = &node.options;
    let mut out = Vec::new();
    for (index, (_, widget)) in node.widgets.iter().enumerate() {
        for state in widget_on_states(doc, widget) {
            let label = labels
                .get(index)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| state.clone());
            out.push(FieldOption { value: state, label });
        }
    }
    out
}

/// The "on" state names of a checkbox/radio field across its widgets.
fn checkbox_states(doc: &Document, node: &FieldNode) -> Vec<String> {
    let mut out = Vec::new();
    for (_, widget) in &node.widgets {
        for state in widget_on_states(doc, widget) {
            if !out.contains(&state) {
                out.push(state);
            }
        }
    }
    out
}

/// Loads `path` (mapping encryption to a clear error) and lists its fields.
pub fn list_fields_in_file(path: &Path, password: Option<&str>) -> PdfResult<Vec<FormFieldInfo>> {
    let doc = docutil::load_document(path, password)?;
    Ok(list_fields(&doc))
}

// ---------------------------------------------------------------------------
// Appearance stream generation
// ---------------------------------------------------------------------------

/// Ensures the AcroForm is an indirect object and returns its id.
fn ensure_acroform(doc: &mut Document) -> PdfResult<ObjectId> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|value| value.as_reference())
        .map_err(|_| PdfError::CorruptPdf("document has no catalog".into()))?;
    let value = doc
        .get_dictionary(catalog_id)
        .map_err(PdfError::from)?
        .get(b"AcroForm")
        .ok()
        .cloned()
        .ok_or_else(|| PdfError::InvalidInput("the document has no AcroForm".into()))?;
    match value {
        Object::Reference(id) => Ok(id),
        Object::Dictionary(dict) => {
            let id = doc.add_object(Object::Dictionary(dict));
            doc.get_dictionary_mut(catalog_id)
                .map_err(PdfError::from)?
                .set("AcroForm", Object::Reference(id));
            Ok(id)
        }
        _ => Err(PdfError::CorruptPdf("/AcroForm is not a dictionary".into())),
    }
}

/// Ensures `owner[key]` exists and is an indirect dictionary; returns its id.
fn ensure_dict_entry(doc: &mut Document, owner: ObjectId, key: &[u8]) -> PdfResult<ObjectId> {
    let existing = doc.get_dictionary(owner).map_err(PdfError::from)?.get(key).ok().cloned();
    match existing {
        Some(Object::Reference(id)) => Ok(id),
        Some(Object::Dictionary(dict)) => {
            let id = doc.add_object(Object::Dictionary(dict));
            set_dict_entry(doc, owner, key, Object::Reference(id))?;
            Ok(id)
        }
        _ => {
            let id = doc.add_object(Object::Dictionary(Dictionary::new()));
            set_dict_entry(doc, owner, key, Object::Reference(id))?;
            Ok(id)
        }
    }
}

/// Finds or creates the Helvetica font used by generated appearances. It is
/// registered in the AcroForm `/DR` (`/Font /Helv`) so unimplemented viewers
/// that ignore per-stream resources still find it.
fn ensure_appearance_font(doc: &mut Document) -> PdfResult<ObjectId> {
    let acro = ensure_acroform(doc)?;
    let dr = ensure_dict_entry(doc, acro, b"DR")?;
    let fonts = ensure_dict_entry(doc, dr, b"Font")?;
    let existing = doc.get_dictionary(fonts).map_err(PdfError::from)?.get(b"Helv").ok().cloned();
    match existing {
        Some(Object::Reference(id)) => {
            if let Ok(Object::Dictionary(_)) = doc.get_object(id) {
                return Ok(id);
            }
        }
        Some(Object::Dictionary(dict)) => {
            let id = doc.add_object(Object::Dictionary(dict));
            set_dict_entry(doc, fonts, b"Helv", Object::Reference(id))?;
            return Ok(id);
        }
        _ => {}
    }
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    set_dict_entry(doc, fonts, b"Helv", Object::Reference(font_id))?;
    Ok(font_id)
}

/// Adds a form XObject carrying a generated appearance.
fn add_appearance_stream(doc: &mut Document, width: f64, height: f64, content: String, font_id: ObjectId) -> ObjectId {
    let stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1i64,
            "BBox" => vec![0.into(), 0.into(), width.into(), height.into()],
            "Resources" => dictionary! {
                "Font" => dictionary! { "Helv" => Object::Reference(font_id) },
            },
        },
        content.into_bytes(),
    );
    doc.add_object(Object::Stream(stream))
}

/// Font size from a `/DA` string (`/Helv 11 Tf`), clamped to something sane.
fn parse_da_size(da: &Option<String>) -> f64 {
    let text = match da {
        Some(text) => text,
        None => return 11.0,
    };
    let tokens: Vec<&str> = text.split_whitespace().collect();
    for index in 1..tokens.len() {
        if tokens[index] == "Tf" {
            if let Ok(size) = tokens[index - 1].parse::<f64>() {
                if size > 0.0 {
                    return size.clamp(4.0, 72.0);
                }
            }
        }
    }
    11.0
}

/// ASCII + WinAnsi-safe literal for a Base14 Helvetica appearance.
///
/// Characters outside ASCII are replaced with `?` **only in the generated
/// appearance**; `/V` always carries the full Unicode string. Replacing them
/// keeps the content stream encodable with the WinAnsi font the appearance
/// declares instead of emitting bytes no Base14 font can render.
fn appearance_text(text: &str) -> String {
    let mapped: String = text
        .chars()
        .map(|c| if c.is_ascii() && c != '\r' && c != '\n' { c } else { '?' })
        .collect();
    docutil::escape_pdf_literal(&mapped)
}

fn format_pt(value: f64) -> String {
    format!("{:.2}", value)
}

/// Splits text into lines that fit an approximate character count.
fn wrap_lines(text: &str, per_line: usize) -> Vec<String> {
    let per_line = per_line.max(1);
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let line: Vec<char> = raw.chars().collect();
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut start = 0;
        while start < line.len() {
            let end = (start + per_line).min(line.len());
            out.push(line[start..end].iter().collect());
            start = end;
        }
    }
    out
}

/// Text field appearance: borderless text for single-line fields, wrapped
/// lines from the top for multiline fields.
fn text_appearance_content(text: &str, width: f64, height: f64, size: f64, multiline: bool) -> String {
    let size = size.max(4.0);
    let usable = (width - 4.0).max(4.0);
    let per_line = (usable / (size * 0.5)).floor().max(1.0) as usize;
    let mut out = String::from("q\nBT\n");
    out.push_str(&format!("/Helv {} Tf\n0 g\n", format_pt(size)));
    if multiline {
        let max_lines = (((height - 4.0).max(1.0)) / (size * 1.15)).floor().max(1.0) as usize;
        let start_y = (height - size - 2.0).max(2.0);
        out.push_str(&format!("2 {} Td\n{} TL\n", format_pt(start_y), format_pt(size * 1.15)));
        for (index, line) in wrap_lines(text, per_line).iter().take(max_lines).enumerate() {
            if index > 0 {
                out.push_str("T*\n");
            }
            out.push_str(&format!("({}) Tj\n", appearance_text(line)));
        }
    } else {
        let single = text.replace(['\r', '\n'], " ");
        let shown: String = single.chars().take(per_line).collect();
        let baseline = ((height - size) / 2.0 + size * 0.2).max(1.0);
        out.push_str(&format!("2 {} Td\n({}) Tj\n", format_pt(baseline), appearance_text(&shown)));
    }
    out.push_str("ET\nQ\n");
    out
}

/// Choice field appearance: text plus a dropdown arrow (combo) or the option
/// list with the selected entries highlighted (list box).
fn choice_appearance_content(
    combo: bool,
    options: &[FieldOption],
    selected: &[String],
    width: f64,
    height: f64,
    size: f64,
) -> String {
    let size = size.max(4.0);
    let mut out = String::from("q\n");
    out.push_str(&format!("0.97 0.98 1 rg\n0.5 0.5 {} {} re f\n", format_pt((width - 1.0).max(0.1)), format_pt((height - 1.0).max(0.1))));
    out.push_str(&format!(
        "0.4 0.45 0.55 RG\n0.7 w\n0.5 0.5 {} {} re S\n",
        format_pt((width - 1.0).max(0.1)),
        format_pt((height - 1.0).max(0.1))
    ));
    if combo {
        let text = selected.join(", ");
        let per_line = (((width - 20.0).max(4.0)) / (size * 0.5)).floor().max(1.0) as usize;
        let shown: String = text.chars().take(per_line).collect();
        let baseline = ((height - size) / 2.0 + size * 0.2).max(1.0);
        out.push_str(&format!("BT\n/Helv {} Tf\n0 g\n2 {} Td\n({}) Tj\nET\n", format_pt(size), format_pt(baseline), appearance_text(&shown)));
        out.push_str(&format!(
            "0.3 0.35 0.45 rg\n{} {} m\n{} {} l\n{} {} l\nf\n",
            format_pt(width - 12.0),
            format_pt(height / 2.0 + 2.0),
            format_pt(width - 6.0),
            format_pt(height / 2.0 + 2.0),
            format_pt(width - 9.0),
            format_pt(height / 2.0 - 2.0)
        ));
    } else {
        // List box: show the options that fit; selected rows get a tint.
        let line_height = size * 1.2;
        let max_lines = (((height - 4.0).max(1.0)) / line_height).floor().max(1.0) as usize;
        let mut y = height - size - 2.0;
        for option in options.iter().take(max_lines) {
            let is_selected = selected.iter().any(|value| value == &option.value);
            if is_selected {
                out.push_str(&format!(
                    "0.78 0.86 1 rg\n2 {} {} {} re f\n",
                    format_pt((y - size * 0.25).max(0.0)),
                    format_pt((width - 14.0).max(1.0)),
                    format_pt(line_height)
                ));
            }
            let shown: String = option.label.chars().take((((width - 8.0).max(4.0)) / (size * 0.5)).floor().max(1.0) as usize).collect();
            out.push_str(&format!(
                "BT\n/Helv {} Tf\n0 g\n3 {} Td\n({}) Tj\nET\n",
                format_pt(size),
                format_pt(y),
                appearance_text(&shown)
            ));
            y -= line_height;
        }
    }
    out.push_str("Q\n");
    out
}

/// A checkbox/radio appearance: white box (square or circle), border, and a
/// check mark / dot when on.
fn button_appearance_content(width: f64, height: f64, on: bool, round: bool) -> String {
    let side = width.min(height).max(4.0);
    let left = (width - side) / 2.0;
    let bottom = (height - side) / 2.0;
    let cx = width / 2.0;
    let cy = height / 2.0;
    let mut out = String::from("q\n1 1 1 rg\n");
    if round {
        let radius = (side - 3.0).max(1.0) / 2.0;
        let k = 0.5523 * radius;
        out.push_str(&circle_path(cx, cy, radius, k));
        out.push_str("f\n0.2 0.2 0.2 RG\n0.8 w\n");
        out.push_str(&circle_path(cx, cy, radius, k));
        out.push_str("S\n");
        if on {
            out.push_str("0.1 0.3 0.8 rg\n");
            out.push_str(&circle_path(cx, cy, radius * 0.5, k * 0.5));
            out.push_str("f\n");
        }
    } else {
        out.push_str(&format!(
            "{} {} {} {} re f\n0.2 0.2 0.2 RG\n0.8 w\n{} {} {} {} re S\n",
            format_pt(left + 0.5),
            format_pt(bottom + 0.5),
            format_pt((side - 1.0).max(0.1)),
            format_pt((side - 1.0).max(0.1)),
            format_pt(left + 0.5),
            format_pt(bottom + 0.5),
            format_pt((side - 1.0).max(0.1)),
            format_pt((side - 1.0).max(0.1))
        ));
        if on {
            out.push_str(&format!(
                "0.05 0.1 0.15 RG\n1.6 w\n1 J\n{} {} m\n{} {} l\n{} {} l\nS\n",
                format_pt(left + side * 0.22),
                format_pt(bottom + side * 0.52),
                format_pt(left + side * 0.42),
                format_pt(bottom + side * 0.28),
                format_pt(left + side * 0.78),
                format_pt(bottom + side * 0.72)
            ));
        }
    }
    out.push_str("Q\n");
    out
}

/// Four cubic Beziers around a circle, as a PDF path (caller draws/fills/Ss).
fn circle_path(cx: f64, cy: f64, radius: f64, k: f64) -> String {
    format!(
        "{} {} m\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n{} {} {} {} {} {} c\n",
        format_pt(cx + radius),
        format_pt(cy),
        format_pt(cx + radius),
        format_pt(cy + k),
        format_pt(cx + k),
        format_pt(cy + radius),
        format_pt(cx),
        format_pt(cy + radius),
        format_pt(cx - k),
        format_pt(cy + radius),
        format_pt(cx - radius),
        format_pt(cy + k),
        format_pt(cx - radius),
        format_pt(cy),
        format_pt(cx - radius),
        format_pt(cy - k),
        format_pt(cx - k),
        format_pt(cy - radius),
        format_pt(cx),
        format_pt(cy - radius),
        format_pt(cx + k),
        format_pt(cy - radius),
        format_pt(cx + radius),
        format_pt(cy - k),
        format_pt(cx + radius),
        format_pt(cy)
    )
}

/// Replaces a widget's `/AP` and sets its `/AS` (for button widgets).
fn set_widget_ap(doc: &mut Document, widget_id: ObjectId, ap: Object) -> PdfResult<()> {
    with_object_dict(doc, widget_id, |dict| dict.set("AP", ap.clone()))
}

fn set_widget_as(doc: &mut Document, widget_id: ObjectId, state: &str) -> PdfResult<()> {
    with_object_dict(doc, widget_id, |dict| dict.set("AS", Object::Name(state.as_bytes().to_vec())))
}

fn widget_rect(doc: &Document, widget_id: ObjectId) -> Option<[f64; 4]> {
    let dict = doc.get_dictionary(widget_id).ok()?;
    rect_of(doc, dict)
}

/// Regenerates the appearance of a text or choice widget for its rect.
fn regenerate_value_appearance(
    doc: &mut Document,
    widget_id: ObjectId,
    content: String,
    font_id: ObjectId,
) -> PdfResult<()> {
    let rect = widget_rect(doc, widget_id).unwrap_or([0.0, 0.0, 120.0, 20.0]);
    let (width, height) = rect_dims(rect);
    let stream_id = add_appearance_stream(doc, width, height, content, font_id);
    set_widget_ap(doc, widget_id, Object::Dictionary(dictionary! { "N" => Object::Reference(stream_id) }))
}

/// Regenerates checkbox/radio appearance streams: one per declared state plus
/// `/Off`, replacing any previous `/AP`.
fn regenerate_button_appearance(
    doc: &mut Document,
    widget_id: ObjectId,
    on_states: &[String],
    on_state: Option<&str>,
    round: bool,
    font_id: ObjectId,
) -> PdfResult<()> {
    let rect = widget_rect(doc, widget_id).unwrap_or([0.0, 0.0, 14.0, 14.0]);
    let (width, height) = rect_dims(rect);
    let off_id = add_appearance_stream(doc, width, height, button_appearance_content(width, height, false, round), font_id);
    let on_id = add_appearance_stream(doc, width, height, button_appearance_content(width, height, true, round), font_id);
    let mut states = Dictionary::new();
    states.set(b"Off".to_vec(), Object::Reference(off_id));
    if on_states.is_empty() {
        states.set(b"Yes".to_vec(), Object::Reference(on_id));
    } else {
        for state in on_states {
            states.set(state.clone().into_bytes(), Object::Reference(on_id));
        }
    }
    let ap = dictionary! { "N" => Object::Dictionary(states) };
    set_widget_ap(doc, widget_id, Object::Dictionary(ap))?;
    match on_state {
        Some(state) if state != "Off" => set_widget_as(doc, widget_id, state)?,
        _ => set_widget_as(doc, widget_id, "Off")?,
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Filling
// ---------------------------------------------------------------------------

/// Applies `values` to `doc` in place.
///
/// The whole value set is validated against the field tree *before* the first
/// byte is written: a request naming a field that does not exist fails
/// cleanly, and a failure can never leave a half-updated form tree behind.
pub fn apply_field_values(doc: &mut Document, values: &[FieldValue]) -> PdfResult<FillReport> {
    let mut report = FillReport::default();
    if values.is_empty() {
        return Ok(report);
    }
    let nodes = collect_field_nodes(doc);
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        by_name.entry(node.full_name.as_str()).or_insert(index);
    }
    for value in values {
        if value.name.is_empty() || !by_name.contains_key(value.name.as_str()) {
            return Err(PdfError::InvalidInput(format!("unknown form field: {}", value.name)));
        }
    }
    let font_id = ensure_appearance_font(doc)?;

    for value in values {
        let node_index = by_name[value.name.as_str()];
        let node = &nodes[node_index];
        let id = node.id;
        let ff = node.ff;
        let ft = node.ft.clone();
        let da = node.da.clone();
        let max_len = node.max_len;
        let widget_ids: Vec<ObjectId> = node.widgets.iter().map(|(widget_id, _)| *widget_id).collect();

        if ff & FLAG_READ_ONLY != 0 {
            report.skipped.push(format!("{}: read-only", value.name));
            continue;
        }
        match ft.as_slice() {
            b"Tx" => {
                let text = value.value.clone();
                if ff & FLAG_MULTILINE == 0 {
                    if let Some(max) = max_len {
                        if text.chars().count() > max.max(0) as usize {
                            report
                                .warnings
                                .push(format!("{}: value is longer than the declared /MaxLen ({max})", value.name));
                        }
                    }
                }
                set_dict_entry(doc, id, b"V", docutil::pdf_text_object(&text))?;
                // Password fields keep the real value in /V but must never
                // draw it: the generated appearance shows one mask character
                // per typed character, like a viewer would.
                let shown = if ff & FLAG_PASSWORD != 0 {
                    "*".repeat(text.chars().count())
                } else {
                    text.clone()
                };
                for widget_id in &widget_ids {
                    let rect = widget_rect(doc, *widget_id).unwrap_or([0.0, 0.0, 120.0, 20.0]);
                    let (width, height) = rect_dims(rect);
                    let content = text_appearance_content(&shown, width, height, parse_da_size(&da), ff & FLAG_MULTILINE != 0);
                    regenerate_value_appearance(doc, *widget_id, content, font_id)?;
                }
                report.filled += 1;
            }
            b"Btn" => {
                if ff & FLAG_PUSHBUTTON != 0 {
                    report.skipped.push(format!("{}: push button", value.name));
                    continue;
                }
                if ff & FLAG_RADIO != 0 {
                    fill_radio(doc, node, value, &widget_ids, font_id, &mut report)?;
                } else {
                    fill_checkbox(doc, node, value, &widget_ids, font_id, &mut report)?;
                }
                report.filled += 1;
            }
            b"Ch" => {
                let selected: Vec<String> = if !value.values.is_empty() {
                    value.values.clone()
                } else if value.value.is_empty() {
                    Vec::new()
                } else {
                    vec![value.value.clone()]
                };
                if ff & FLAG_MULTI_SELECT == 0 && selected.len() > 1 {
                    report
                        .warnings
                        .push(format!("{}: field does not allow multiple values; only the first is written", value.name));
                }
                let effective: Vec<String> = if ff & FLAG_MULTI_SELECT == 0 {
                    selected.iter().take(1).cloned().collect()
                } else {
                    selected.clone()
                };
                let v_object = if ff & FLAG_MULTI_SELECT != 0 {
                    Object::Array(effective.iter().map(|entry| docutil::pdf_text_object(entry)).collect())
                } else if effective.is_empty() {
                    docutil::pdf_text_object("")
                } else {
                    docutil::pdf_text_object(&effective[0])
                };
                set_dict_entry(doc, id, b"V", v_object)?;
                for widget_id in &widget_ids {
                    let rect = widget_rect(doc, *widget_id).unwrap_or([0.0, 0.0, 120.0, 20.0]);
                    let (width, height) = rect_dims(rect);
                    let content = choice_appearance_content(
                        ff & FLAG_COMBO != 0,
                        &node.options,
                        &effective,
                        width,
                        height,
                        parse_da_size(&da),
                    );
                    regenerate_value_appearance(doc, *widget_id, content, font_id)?;
                }
                report.filled += 1;
            }
            b"Sig" => report.skipped.push(format!("{}: signature field", value.name)),
            _ => report.skipped.push(format!("{}: unsupported field type", value.name)),
        }
    }

    // Compatibility safety net: viewers that can regenerate appearances should
    // do so for any field whose appearance stream we did not or could not
    // rebuild exactly the way the producer would have.
    if report.filled > 0 {
        if let Ok(acro) = ensure_acroform(doc) {
            let _ = set_dict_entry(doc, acro, b"NeedAppearances", Object::Boolean(true));
        }
    }
    Ok(report)
}

fn fill_checkbox(
    doc: &mut Document,
    node: &FieldNode,
    value: &FieldValue,
    widget_ids: &[ObjectId],
    font_id: ObjectId,
    report: &mut FillReport,
) -> PdfResult<()> {
    let states = checkbox_states(doc, node);
    let raw = value.value.trim().to_string();
    let lower = raw.to_lowercase();
    let chosen: String = if raw.is_empty() || matches!(lower.as_str(), "off" | "false" | "0" | "no" | "unchecked") {
        "Off".to_string()
    } else if matches!(lower.as_str(), "on" | "true" | "1" | "yes" | "checked") {
        states.first().cloned().unwrap_or_else(|| "Yes".to_string())
    } else if states.iter().any(|state| state == &raw) {
        raw.clone()
    } else {
        report
            .warnings
            .push(format!("{}: unknown checkbox state '{raw}', using the first declared state", value.name));
        states.first().cloned().unwrap_or_else(|| "Yes".to_string())
    };
    set_dict_entry(doc, node.id, b"V", Object::Name(chosen.as_bytes().to_vec()))?;
    for widget_id in widget_ids {
        regenerate_button_appearance(doc, *widget_id, &states, Some(&chosen), false, font_id)?;
    }
    Ok(())
}

fn fill_radio(
    doc: &mut Document,
    node: &FieldNode,
    value: &FieldValue,
    widget_ids: &[ObjectId],
    font_id: ObjectId,
    report: &mut FillReport,
) -> PdfResult<()> {
    // Collect per-widget states first: each widget owns one export value.
    let per_widget: Vec<Vec<String>> = node
        .widgets
        .iter()
        .map(|(_, widget)| widget_on_states(doc, widget))
        .collect();
    let all_states: Vec<String> = per_widget.iter().flatten().cloned().collect();
    let raw = value.value.trim().to_string();
    let lower = raw.to_lowercase();
    let chosen: Option<String> = if raw.is_empty() || matches!(lower.as_str(), "off" | "false" | "0" | "no" | "unchecked") {
        None
    } else if all_states.iter().any(|state| state == &raw) {
        Some(raw.clone())
    } else if all_states.is_empty() {
        // The document declares no appearance states (some producers, and the
        // built-in form creator, only ship a single stream). Writing the state
        // the caller asked for is the only honest option; the regenerated
        // appearance below gives that state a stream so the value is visible.
        Some(raw.clone())
    } else {
        report
            .warnings
            .push(format!("{}: '{raw}' is not a declared radio state; leaving the group off", value.name));
        None
    };
    match &chosen {
        Some(state) => set_dict_entry(doc, node.id, b"V", Object::Name(state.as_bytes().to_vec()))?,
        None => set_dict_entry(doc, node.id, b"V", Object::Name(b"Off".to_vec()))?,
    }
    for (index, widget_id) in widget_ids.iter().enumerate() {
        let mut states = per_widget.get(index).cloned().unwrap_or_default();
        let is_chosen = match &chosen {
            Some(state) => states.iter().any(|entry| entry == state) || states.is_empty(),
            None => false,
        };
        if states.is_empty() {
            if let Some(state) = &chosen {
                states.push(state.clone());
            }
        }
        let on_state = if is_chosen { chosen.as_deref() } else { None };
        regenerate_button_appearance(doc, *widget_id, &states, on_state, true, font_id)?;
    }
    Ok(())
}

/// Convenience wrapper over the byte level signature: parse, fill, serialize.
///
/// Note for callers holding a password protected document: this helper has no
/// password parameter, so encrypted inputs must be loaded through
/// `docutil::load_document` and filled via `apply_field_values`.
pub fn fill_fields(pdf: &[u8], values: &[FieldValue]) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::from_lopdf(error, None))?;
    apply_field_values(&mut doc, values)?;
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(PdfError::from_io)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Validation (no PDF JavaScript is ever executed)
// ---------------------------------------------------------------------------

/// Checks the provable constraints of `values` against the form in `doc`.
pub fn validate_fields(doc: &Document, values: &[FieldValue]) -> Vec<FieldIssue> {
    let nodes = collect_field_nodes(doc);
    let mut by_name: HashMap<&str, &FieldNode> = HashMap::new();
    for node in &nodes {
        by_name.entry(node.full_name.as_str()).or_insert(node);
    }
    let mut issues: Vec<FieldIssue> = Vec::new();

    // Required fields: either a provided value or the current /V must satisfy
    // the field. A checkbox counts as filled only in a non-Off state.
    for node in &nodes {
        if node.ff & FLAG_REQUIRED == 0 {
            continue;
        }
        let provided = values.iter().find(|value| value.name == node.full_name);
        let effective: Vec<String> = match provided {
            Some(value) if !value.values.is_empty() => value.values.clone(),
            Some(value) if !value.value.is_empty() => vec![value.value.clone()],
            Some(_) => Vec::new(),
            None => node_values(doc, node),
        };
        let field_type = node_field_type(node);
        let filled = match field_type {
            "checkbox" => effective
                .iter()
                .any(|value| !value.is_empty() && !value.eq_ignore_ascii_case("off")),
            _ => effective.iter().any(|value| !value.is_empty()),
        };
        if !filled {
            issues.push(FieldIssue {
                field: node.full_name.clone(),
                code: "required".into(),
                severity: "error".into(),
                message: format!("The required field '{}' has no value.", node.full_name),
            });
        }
    }

    for value in values {
        let node = match by_name.get(value.name.as_str()) {
            Some(node) => *node,
            None => {
                issues.push(FieldIssue {
                    field: value.name.clone(),
                    code: "unknown_field".into(),
                    severity: "error".into(),
                    message: format!("The document has no form field named '{}'.", value.name),
                });
                continue;
            }
        };
        if node.ff & FLAG_READ_ONLY != 0 && (!value.value.is_empty() || !value.values.is_empty()) {
            issues.push(FieldIssue {
                field: value.name.clone(),
                code: "read_only".into(),
                severity: "error".into(),
                message: format!("'{}' is read-only; a value cannot be accepted for it.", value.name),
            });
        }
        if node.dict.get(b"AA").is_ok() {
            issues.push(FieldIssue {
                field: value.name.clone(),
                code: "scripted_format".into(),
                severity: "warning".into(),
                message: format!(
                    "'{}' declares a JavaScript format/validation action. JavaScript is never executed here, so only the structural checks below were applied.",
                    value.name
                ),
            });
        }
        let field_type = node_field_type(node);
        match field_type {
            "text" => {
                let text = if value.values.is_empty() { value.value.clone() } else { value.values.join(", ") };
                if let Some(max) = node.max_len {
                    if text.chars().count() > max.max(0) as usize {
                        issues.push(FieldIssue {
                            field: value.name.clone(),
                            code: "max_length".into(),
                            severity: "error".into(),
                            message: format!(
                                "'{}' accepts at most {} characters; the value has {}.",
                                value.name,
                                max,
                                text.chars().count()
                            ),
                        });
                    }
                }
                if !text.is_empty() {
                    check_name_based_format(doc, node, &value.name, &text, &mut issues);
                }
            }
            "checkbox" => {
                let raw = value.value.trim();
                if !raw.is_empty() {
                    let states = checkbox_states(doc, node);
                    let lower = raw.to_lowercase();
                    let boolean_word = matches!(
                        lower.as_str(),
                        "off" | "false" | "0" | "no" | "unchecked" | "on" | "true" | "1" | "yes" | "checked"
                    );
                    if !boolean_word && !states.iter().any(|state| state == raw) {
                        issues.push(FieldIssue {
                            field: value.name.clone(),
                            code: "option_not_in_list".into(),
                            severity: "error".into(),
                            message: format!("'{raw}' is not a declared state of the checkbox '{}'.", value.name),
                        });
                    }
                }
            }
            "radio" => {
                let raw = value.value.trim();
                if !raw.is_empty() && !raw.eq_ignore_ascii_case("off") {
                    let states = checkbox_states(doc, node);
                    // A group with no declared states can only be validated
                    // against itself; staying silent would be dishonest, so it
                    // is reported as "cannot check" via a warning instead.
                    if states.is_empty() {
                        issues.push(FieldIssue {
                            field: value.name.clone(),
                            code: "scripted_format".into(),
                            severity: "warning".into(),
                            message: format!(
                                "'{}' declares no radio appearance states; the value cannot be verified against the file.",
                                value.name
                            ),
                        });
                    } else if !states.iter().any(|state| state == raw) {
                        issues.push(FieldIssue {
                            field: value.name.clone(),
                            code: "option_not_in_list".into(),
                            severity: "error".into(),
                            message: format!(
                                "'{raw}' is not a declared option of '{}' (declared: {}).",
                                value.name,
                                states.join(", ")
                            ),
                        });
                    }
                }
            }
            "choice" => {
                let selected: Vec<String> = if !value.values.is_empty() {
                    value.values.clone()
                } else if value.value.is_empty() {
                    Vec::new()
                } else {
                    vec![value.value.clone()]
                };
                if node.ff & FLAG_MULTI_SELECT == 0 && selected.len() > 1 {
                    issues.push(FieldIssue {
                        field: value.name.clone(),
                        code: "multi_select_not_allowed".into(),
                        severity: "error".into(),
                        message: format!("'{}' does not allow multiple selections.", value.name),
                    });
                }
                if node.ff & FLAG_EDIT == 0 {
                    for entry in &selected {
                        if !node.options.iter().any(|option| &option.value == entry) {
                            issues.push(FieldIssue {
                                field: value.name.clone(),
                                code: "option_not_in_list".into(),
                                severity: "error".into(),
                                message: format!(
                                    "'{entry}' is not in the option list of '{}' ({}).",
                                    value.name,
                                    node
                                        .options
                                        .iter()
                                        .map(|option| option.value.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                            });
                        }
                    }
                }
            }
            "pushbutton" | "signature"
                if (!value.value.is_empty() || !value.values.is_empty()) => {
                    issues.push(FieldIssue {
                        field: value.name.clone(),
                        code: "not_fillable".into(),
                        severity: "error".into(),
                        message: format!("'{}' is a {} field and cannot take a value.", value.name, field_type),
                    });
                }
            _ => {}
        }
    }
    issues
}

/// Heuristic date/number checks driven by the field name or tooltip.
///
/// These never run scripts; a value that does not look like a date/number is
/// only a `warning`, because the name is a guess, not a contract.
fn check_name_based_format(doc: &Document, node: &FieldNode, name: &str, text: &str, issues: &mut Vec<FieldIssue>) {
    let tooltip = dict_text(doc, &node.dict, b"TU").unwrap_or_default();
    let hint = format!("{} {}", name, tooltip).to_lowercase();
    let date_hint = ["date", "tarih", "birth", "dogum", "doğum", "validuntil", "expiry", "geçerlilik"]
        .iter()
        .any(|needle| hint.contains(needle));
    let number_hint = ["amount", "total", "price", "tutar", "fiyat", "toplam", "number", "sayı", "sayi", "quantity", "adet"]
        .iter()
        .any(|needle| hint.contains(needle));
    if date_hint && !looks_like_date(text) {
        issues.push(FieldIssue {
            field: name.to_string(),
            code: "invalid_date".into(),
            severity: "warning".into(),
            message: format!("'{text}' does not look like a date; the field name suggests one (checked without scripts)."),
        });
    } else if number_hint && !looks_like_number(text) {
        issues.push(FieldIssue {
            field: name.to_string(),
            code: "invalid_number".into(),
            severity: "warning".into(),
            message: format!("'{text}' does not look like a number; the field name suggests one (checked without scripts)."),
        });
    }
}

/// ISO (`yyyy-mm-dd`), dotted/slashed day-month-year (both orders), and
/// compact `yyyymmdd` - the common shapes, with range checks.
fn looks_like_date(value: &str) -> bool {
    // ISO date-times are accepted on their date part; the time part is not
    // validated because nothing here executes a format script.
    let text = value.trim().split(['T', 't', ' ']).next().unwrap_or(value.trim());
    if text.is_empty() {
        return false;
    }
    let separators = ['.', '/', '-'];
    for separator in separators {
        let parts: Vec<&str> = text.split(separator).collect();
        if parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())) {
            let numbers: Vec<u32> = parts.iter().filter_map(|part| part.parse::<u32>().ok()).collect();
            if numbers.len() != 3 {
                continue;
            }
            let plausible = |year: u32, month: u32, day: u32| {
                (1..=12).contains(&month) && (1..=31).contains(&day) && year <= 9999
            };
            let (a, b, c) = (numbers[0], numbers[1], numbers[2]);
            // y-m-d or d-m-y / m-d-y; accept when either reading is plausible.
            if plausible(a, b, c) || plausible(c, b, a) || plausible(c, a, b) {
                return true;
            }
        }
    }
    if text.len() == 8 && text.chars().all(|c| c.is_ascii_digit()) {
        let year: u32 = text[0..4].parse().unwrap_or(0);
        let month: u32 = text[4..6].parse().unwrap_or(0);
        let day: u32 = text[6..8].parse().unwrap_or(0);
        return (1..=12).contains(&month) && (1..=31).contains(&day) && year <= 9999;
    }
    false
}

/// Accepts plain numbers with either decimal separator, optional sign, and
/// dot/comma thousands groups. Deliberately conservative.
fn looks_like_number(value: &str) -> bool {
    let text = value.trim().replace(' ', "");
    if text.is_empty() {
        return false;
    }
    let unsigned = text.trim_start_matches(['+', '-']);
    if unsigned.is_empty() {
        return false;
    }
    if unsigned.matches(['.', ',']).count() <= 1 {
        return unsigned
            .chars()
            .filter(|c| *c != '.' && *c != ',')
            .all(|c| c.is_ascii_digit())
            && unsigned.chars().any(|c| c.is_ascii_digit());
    }
    // Thousands groups: 1.234.567 or 1,234,567 (consistent separator).
    let (separator, others): (char, &[char]) = if unsigned.contains('.') { ('.', &['.', ',']) } else { (',', &['.', ',']) };
    let parts: Vec<&str> = unsigned.split(separator).collect();
    !parts.is_empty()
        && parts.iter().all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()) && !others.iter().any(|other| part.contains(*other))
        })
}

// ---------------------------------------------------------------------------
// Page object listing
// ---------------------------------------------------------------------------

/// One annotation, widget or image placement on a page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PageObjectInfo {
    pub page: u32,
    /// Stable position within the page's object list (annotations first, then
    /// drawn images in content order). This is the key `ObjectEdit` uses.
    pub index: u32,
    /// `annotation` | `widget` | `image`.
    pub kind: String,
    /// Annotation `/Subtype`, or `Image`.
    pub subtype: String,
    /// `"object generation"` for indirect annotations; `None` for inline ones.
    pub id: Option<String>,
    /// Page-space bounding box, bottom-left origin (`/Rect`, or the placed
    /// image's bounding box).
    pub rect: [f64; 4],
    /// Placement matrix for images (unit square -> page space).
    pub matrix: Option<[f64; 6]>,
    /// Resource name of the image (`/Im0`), when the object is an image.
    pub resource_name: Option<String>,
    /// Field name for widget annotations.
    pub field_name: Option<String>,
    /// Annotation `/Contents` (truncated).
    pub contents: String,
    /// Annotation `/F` flags.
    pub flags: i64,
    pub hidden: bool,
    pub tab_order: Option<u32>,
}

/// One `cm` + `/Name Do` placement of an image XObject.
#[derive(Debug, Clone)]
struct ImagePlacement {
    stream_id: ObjectId,
    name: String,
    /// Full CTM at `Do` time (unit square -> page space).
    matrix: Matrix,
    /// The `cm` that produced the current CTM, when it is patchable in place.
    last_cm: Option<(Matrix, Vec<(usize, usize)>)>,
    /// Byte range of the `Do` operator (and of the name token) in the stream.
    name_range: (usize, usize),
    do_range: (usize, usize),
}

#[derive(Debug, Clone, Copy)]
enum AnnotsStorage {
    Inline,
    Array(ObjectId),
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

/// Stable handle for one annotation entry (indirect or inline).
#[derive(Debug, Clone, Copy)]
enum AnnotHandle {
    Ref(ObjectId),
    Inline { page_id: ObjectId, position: usize },
}

#[derive(Debug, Clone)]
enum PageObjectEntry {
    Annot { handle: AnnotHandle, dict: Dictionary },
    Image { placement: ImagePlacement },
}

/// Enumerates objects on a page in the exact order `list_page_objects`
/// reports them, so `ObjectEdit.index` is stable across list and edit calls.
fn page_object_entries(doc: &Document, page_id: ObjectId) -> Vec<PageObjectEntry> {
    let mut out = Vec::new();
    if let Some((_, items)) = annots_storage(doc, page_id) {
        for (position, entry) in items.iter().enumerate() {
            match entry {
                Object::Reference(id) => {
                    if let Ok(dict) = doc.get_dictionary(*id) {
                        out.push(PageObjectEntry::Annot {
                            handle: AnnotHandle::Ref(*id),
                            dict: dict.clone(),
                        });
                    }
                }
                Object::Dictionary(dict) => {
                    // Inline dictionaries are addressed by their position in
                    // the page's annotation array, whichever way it is stored.
                    out.push(PageObjectEntry::Annot {
                        handle: AnnotHandle::Inline { page_id, position },
                        dict: dict.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    for placement in image_placements(doc, page_id) {
        out.push(PageObjectEntry::Image { placement });
    }
    out
}

/// Lists every annotation, widget and drawn image on every page.
pub fn list_page_objects(doc: &Document) -> Vec<PageObjectInfo> {
    let pages = page_map(doc);
    let mut out = Vec::new();
    for (page_number, page_id) in &pages {
        let tabs = tab_order_map(doc, *page_id);
        for (index, entry) in page_object_entries(doc, *page_id).into_iter().enumerate() {
            match entry {
                PageObjectEntry::Annot { handle, dict } => {
                    let subtype = dict_name(&dict, b"Subtype")
                        .map(|name| String::from_utf8_lossy(&name).to_string())
                        .unwrap_or_else(|| "Annotation".to_string());
                    let flags = dict_i64(&dict, b"F").unwrap_or(0);
                    let id = match handle {
                        AnnotHandle::Ref(id) => Some(format!("{} {}", id.0, id.1)),
                        AnnotHandle::Inline { .. } => None,
                    };
                    let contents = dict_text(doc, &dict, b"Contents").unwrap_or_default();
                    out.push(PageObjectInfo {
                        page: *page_number,
                        index: index as u32,
                        kind: if subtype == "Widget" { "widget".into() } else { "annotation".into() },
                        subtype,
                        id,
                        rect: rect_of(doc, &dict).unwrap_or([0.0, 0.0, 0.0, 0.0]),
                        matrix: None,
                        resource_name: None,
                        field_name: dict_text(doc, &dict, b"T"),
                        contents: contents.chars().take(160).collect(),
                        flags,
                        hidden: flags & 2 != 0,
                        tab_order: match handle {
                            AnnotHandle::Ref(id) => tabs.get(&id).copied(),
                            AnnotHandle::Inline { .. } => None,
                        },
                    });
                }
                PageObjectEntry::Image { placement } => {
                    out.push(PageObjectInfo {
                        page: *page_number,
                        index: index as u32,
                        kind: "image".into(),
                        subtype: "Image".into(),
                        id: Some(format!("{} {}", placement.stream_id.0, placement.stream_id.1)),
                        rect: matrix_bbox(placement.matrix),
                        matrix: Some(placement.matrix.0),
                        resource_name: Some(placement.name.clone()),
                        field_name: None,
                        contents: String::new(),
                        flags: 0,
                        hidden: false,
                        tab_order: None,
                    });
                }
            }
        }
    }
    out
}

/// Loads `path` (mapping encryption to a clear error) and lists page objects.
pub fn list_page_objects_in_file(path: &Path, password: Option<&str>) -> PdfResult<Vec<PageObjectInfo>> {
    let doc = docutil::load_document(path, password)?;
    Ok(list_page_objects(&doc))
}

fn matrix_bbox(matrix: Matrix) -> [f64; 4] {
    let corners = [
        matrix.apply(0.0, 0.0),
        matrix.apply(1.0, 0.0),
        matrix.apply(0.0, 1.0),
        matrix.apply(1.0, 1.0),
    ];
    let min_x = corners.iter().map(|corner| corner.0).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|corner| corner.0).fold(f64::MIN, f64::max);
    let min_y = corners.iter().map(|corner| corner.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|corner| corner.1).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x, max_y]
}

// ---------------------------------------------------------------------------
// A tiny content-stream scanner
//
// Full content stream parsing is out of scope; this scanner understands just
// enough to follow the CTM through q/Q/cm and find `/Name Do`, while skipping
// strings, arrays and dictionaries so nothing inside them is mistaken for an
// operator. It never recurses and never allocates per byte.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
    Number(f64),
    Name(Vec<u8>),
    Operator(Vec<u8>),
    Other,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

fn is_content_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_content_delimiter(byte: u8) -> bool {
    is_content_whitespace(byte) || matches!(byte, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn skip_literal_string(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    let mut depth = 1i32;
    while index < bytes.len() && depth > 0 {
        match bytes[index] {
            b'\\' => index += 2,
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth -= 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    index.min(bytes.len())
}

fn skip_hex_string(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < bytes.len() && bytes[index] != b'>' {
        index += 1;
    }
    (index + 1).min(bytes.len())
}

/// Skips a balanced `[...]` or `<<...>>` construct.
fn skip_balanced(bytes: &[u8], start: usize) -> usize {
    let mut stack: Vec<u8> = Vec::new();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => {
                index = skip_literal_string(bytes, index);
                continue;
            }
            b'<' if bytes.get(index + 1) == Some(&b'<') => {
                stack.push(b'<');
                index += 2;
                continue;
            }
            b'<' => {
                index = skip_hex_string(bytes, index);
                continue;
            }
            b'[' => {
                stack.push(b'[');
                index += 1;
                continue;
            }
            b']' => {
                stack.pop();
                index += 1;
                if stack.is_empty() {
                    return index;
                }
                continue;
            }
            b'>' if bytes.get(index + 1) == Some(&b'>') => {
                stack.pop();
                index += 2;
                if stack.is_empty() {
                    return index;
                }
                continue;
            }
            b'%' => {
                while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
                    index += 1;
                }
                continue;
            }
            _ => index += 1,
        }
    }
    index.min(bytes.len())
}

fn scan_content(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if is_content_whitespace(byte) {
            index += 1;
            continue;
        }
        if byte == b'%' {
            while index < bytes.len() && bytes[index] != b'\n' && bytes[index] != b'\r' {
                index += 1;
            }
            continue;
        }
        if byte == b'(' {
            let start = index;
            index = skip_literal_string(bytes, index);
            tokens.push(Token { kind: TokenKind::Other, start, end: index });
            continue;
        }
        if byte == b'[' || (byte == b'<' && bytes.get(index + 1) == Some(&b'<')) {
            let start = index;
            index = skip_balanced(bytes, index);
            tokens.push(Token { kind: TokenKind::Other, start, end: index });
            continue;
        }
        if byte == b'<' {
            let start = index;
            index = skip_hex_string(bytes, index);
            tokens.push(Token { kind: TokenKind::Other, start, end: index });
            continue;
        }
        if byte == b'/' {
            let start = index;
            index += 1;
            while index < bytes.len() && !is_content_delimiter(bytes[index]) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Name(bytes[start + 1..index].to_vec()),
                start,
                end: index,
            });
            continue;
        }
        if byte.is_ascii_digit() || byte == b'+' || byte == b'-' || byte == b'.' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_digit() || matches!(bytes[index], b'+' | b'-' | b'.' | b'e' | b'E'))
            {
                index += 1;
            }
            let parsed = std::str::from_utf8(&bytes[start..index])
                .ok()
                .and_then(|text| text.parse::<f64>().ok());
            match parsed {
                Some(value) => tokens.push(Token { kind: TokenKind::Number(value), start, end: index }),
                None => tokens.push(Token { kind: TokenKind::Other, start, end: index }),
            }
            continue;
        }
        if !is_content_delimiter(byte) {
            let start = index;
            while index < bytes.len() && !is_content_delimiter(bytes[index]) {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Operator(bytes[start..index].to_vec()),
                start,
                end: index,
            });
            continue;
        }
        index += 1;
    }
    // Drop inline image payloads (`BI ... ID <binary> EI`): the binary bytes
    // between ID and EI are not content stream syntax and must never be
    // interpreted as `/Name Do` or `cm` operands.
    let mut filtered = Vec::with_capacity(tokens.len());
    let mut inline = false;
    for token in tokens {
        if inline {
            if let TokenKind::Operator(operator) = &token.kind {
                if operator == b"EI" {
                    inline = false;
                }
            }
            continue;
        }
        if let TokenKind::Operator(operator) = &token.kind {
            if operator == b"BI" {
                inline = true;
                continue;
            }
        }
        filtered.push(token);
    }
    filtered
}

/// Resource names of image XObjects referenced by a page.
fn page_image_xobjects(doc: &Document, page_id: ObjectId) -> Vec<(String, ObjectId)> {
    let resources = match doc.get_page_resources(page_id) {
        Ok((Some(resources), _)) => resources.clone(),
        _ => match doc.get_dictionary(page_id) {
            Ok(page) => match resolve_dict(doc, page.get(b"Resources").ok()) {
                Some(resources) => resources,
                None => return Vec::new(),
            },
            Err(_) => return Vec::new(),
        },
    };
    let xobjects = match resolve_dict(doc, resources.get(b"XObject").ok()) {
        Some(xobjects) => xobjects,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    for (key, value) in xobjects.iter() {
        let id = match value {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let is_image = matches!(
            doc.get_object(id),
            Ok(Object::Stream(stream))
                if stream.dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Image".as_slice())
        );
        if is_image {
            out.push((String::from_utf8_lossy(key).to_string(), id));
        }
    }
    out
}

fn decoded_stream_bytes(doc: &Document, stream_id: ObjectId) -> Option<Vec<u8>> {
    match doc.get_object(stream_id) {
        Ok(Object::Stream(stream)) => stream.decompressed_content().ok(),
        _ => None,
    }
}

/// Finds every image draw in a page's content streams, tracking the CTM.
fn image_placements(doc: &Document, page_id: ObjectId) -> Vec<ImagePlacement> {
    let images: HashMap<String, ObjectId> = page_image_xobjects(doc, page_id).into_iter().collect();
    if images.is_empty() {
        return Vec::new();
    }
    let stream_ids = doc.get_page_contents(page_id);
    let mut out = Vec::new();
    let mut ctm = Matrix::IDENTITY;
    let mut last_cm: Option<(Matrix, Vec<(usize, usize)>)> = None;
    let mut stack: Vec<(Matrix, Option<(Matrix, Vec<(usize, usize)>)>)> = Vec::new();

    for stream_id in stream_ids {
        let bytes = match decoded_stream_bytes(doc, stream_id) {
            Some(bytes) => bytes,
            None => continue,
        };
        let tokens = scan_content(&bytes);
        for (index, token) in tokens.iter().enumerate() {
            let operator = match &token.kind {
                TokenKind::Operator(operator) => operator.as_slice(),
                _ => continue,
            };
            match operator {
                b"q" => stack.push((ctm, last_cm.clone())),
                b"Q" => {
                    if let Some((previous, previous_cm)) = stack.pop() {
                        ctm = previous;
                        last_cm = previous_cm;
                    }
                }
                b"cm" => {
                    if index >= 6 {
                        let operands = &tokens[index - 6..index];
                        let values: Option<Vec<f64>> = operands
                            .iter()
                            .map(|operand| match operand.kind {
                                TokenKind::Number(value) => Some(value),
                                _ => None,
                            })
                            .collect();
                        if let Some(values) = values {
                            let matrix = Matrix([values[0], values[1], values[2], values[3], values[4], values[5]]);
                            ctm = matrix.mul(ctm);
                            last_cm = Some((
                                matrix,
                                operands.iter().map(|operand| (operand.start, operand.end)).collect(),
                            ));
                        }
                    }
                }
                b"Do"
                    if index >= 1 => {
                        if let TokenKind::Name(name) = &tokens[index - 1].kind {
                            let name = String::from_utf8_lossy(name).to_string();
                            if images.contains_key(&name) {
                                out.push(ImagePlacement {
                                    stream_id,
                                    name,
                                    matrix: ctm,
                                    last_cm: last_cm.clone(),
                                    name_range: (tokens[index - 1].start, tokens[index - 1].end),
                                    do_range: (token.start, token.end),
                                });
                            }
                        }
                    }
                _ => {}
            }
        }
    }
    out
}

fn format_number(value: f64) -> String {
    let rounded = value.round();
    if (value - rounded).abs() < 1e-9 {
        return format!("{}", rounded as i64);
    }
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Replaces the six matrix operands with new values, byte-identical elsewhere.
fn rewrite_matrix_bytes(bytes: &[u8], ranges: &[(usize, usize)], values: [f64; 6]) -> Option<Vec<u8>> {
    if ranges.len() != 6 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() + 32);
    let mut cursor = 0usize;
    for (index, (start, end)) in ranges.iter().enumerate() {
        if *start < cursor || *end > bytes.len() || start >= end {
            return None;
        }
        out.extend_from_slice(&bytes[cursor..*start]);
        out.extend_from_slice(format_number(values[index]).as_bytes());
        cursor = *end;
    }
    out.extend_from_slice(&bytes[cursor..]);
    Some(out)
}

fn insert_matrix_bytes(bytes: &[u8], at: usize, values: [f64; 6]) -> Option<Vec<u8>> {
    if at > bytes.len() {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() + 64);
    out.extend_from_slice(&bytes[..at]);
    out.extend_from_slice(
        format!(
            "{} {} {} {} {} {} cm\n",
            format_number(values[0]),
            format_number(values[1]),
            format_number(values[2]),
            format_number(values[3]),
            format_number(values[4]),
            format_number(values[5])
        )
        .as_bytes(),
    );
    out.extend_from_slice(&bytes[at..]);
    Some(out)
}

// ---------------------------------------------------------------------------
// Object editing
// ---------------------------------------------------------------------------

/// One edit requested for an object listed by `list_page_objects`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action")]
pub enum ObjectAction {
    /// Translate by (dx, dy) page points.
    Move { dx: f64, dy: f64 },
    /// New page-space rectangle `[x0, y0, x1, y1]` (bottom-left origin).
    Resize { rect: [f64; 4] },
    Delete,
    /// Rotate around the object's center, in degrees.
    Rotate { degrees: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectEdit {
    pub page: u32,
    pub index: u32,
    pub action: ObjectAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EditReport {
    pub edited: u32,
    pub deleted: u32,
    pub warnings: Vec<String>,
}

/// What the caller asked for on an image placement. Matrix intents compose
/// across edits; a Delete intent is terminal (further edits are ignored).
enum ImageIntent {
    Matrix(ImagePlacement, Matrix),
    Delete(ImagePlacement),
}

/// One byte-level operation on a content stream, anchored to offsets in the
/// original decoded stream. Applied from the highest offset downwards.
enum StreamOp {
    Replace { ranges: Vec<(usize, usize)>, values: [f64; 6] },
    Insert { at: usize, values: [f64; 6] },
    Blank { start: usize, end: usize },
}

/// Applies `edits` to `doc` in place.
///
/// Targets are resolved against a snapshot of the original object lists, so
/// edits never shift under each other: deleting object 1 does not make a
/// later edit of object 3 hit the wrong annotation.
pub fn apply_object_edits(doc: &mut Document, edits: &[ObjectEdit]) -> PdfResult<EditReport> {
    let mut report = EditReport::default();
    if edits.is_empty() {
        return Ok(report);
    }
    let pages = page_map(doc);
    // Snapshot the target lists before mutating anything.
    let mut snapshots: HashMap<u32, Vec<PageObjectEntry>> = HashMap::new();
    for edit in edits {
        if let std::collections::hash_map::Entry::Vacant(e) = snapshots.entry(edit.page) {
            let page_id = pages
                .get(&edit.page)
                .copied()
                .ok_or(PdfError::RangeOutOfBounds)?;
            e.insert(page_object_entries(doc, page_id));
        }
    }
    // Validate all indexes and parameters before writing.
    for edit in edits {
        let entries = &snapshots[&edit.page];
        if entries.get(edit.index as usize).is_none() {
            return Err(PdfError::InvalidInput(format!(
                "page {} has no object at index {}",
                edit.page, edit.index
            )));
        }
        validate_action(&edit.action)?;
    }

    let mut image_intents: HashMap<(u32, u32), ImageIntent> = HashMap::new();
    let mut deletions: Vec<(AnnotHandle, Vec<ObjectId>)> = Vec::new();

    for edit in edits {
        let entries = snapshots.get(&edit.page).cloned().unwrap_or_default();
        let entry = match entries.get(edit.index as usize) {
            Some(entry) => entry.clone(),
            None => continue,
        };
        let key = (edit.page, edit.index);
        match entry {
            PageObjectEntry::Annot { handle, dict } => match &edit.action {
                ObjectAction::Move { dx, dy } => {
                    // Read the *current* rect, not the snapshot: two moves in
                    // one request must compose.
                    let fresh = annot_dict_snapshot(doc, handle).unwrap_or_else(|| dict.clone());
                    let rect = rect_of(doc, &fresh).unwrap_or([0.0, 0.0, 0.0, 0.0]);
                    let moved = [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy];
                    with_annot_dict(doc, handle, |target| target.set("Rect", rect_to_object(moved)))?;
                    report.edited += 1;
                }
                ObjectAction::Resize { rect } => {
                    let normalized = rect_normalize(*rect);
                    if normalized[2] - normalized[0] < 1.0 || normalized[3] - normalized[1] < 1.0 {
                        return Err(PdfError::InvalidInput("resize rectangle is degenerate".into()));
                    }
                    with_annot_dict(doc, handle, |target| target.set("Rect", rect_to_object(normalized)))?;
                    report.edited += 1;
                }
                ObjectAction::Rotate { degrees } => {
                    rotate_annotation(doc, handle, *degrees)?;
                    report.edited += 1;
                }
                ObjectAction::Delete => {
                    let ids = match handle {
                        AnnotHandle::Ref(id) => vec![id],
                        AnnotHandle::Inline { .. } => Vec::new(),
                    };
                    deletions.push((handle, ids));
                    report.deleted += 1;
                }
            },
            PageObjectEntry::Image { placement } => {
                if matches!(image_intents.get(&key), Some(ImageIntent::Delete(_))) {
                    // The draw is already marked for removal; later requests
                    // for the same object have nothing left to act on.
                    continue;
                }
                let mut desired = match image_intents.get(&key) {
                    Some(ImageIntent::Matrix(_, desired)) => *desired,
                    _ => placement.matrix,
                };
                match &edit.action {
                    ObjectAction::Move { dx, dy } => {
                        desired = Matrix::translate(*dx, *dy).mul(desired);
                        image_intents.insert(key, ImageIntent::Matrix(placement, desired));
                        report.edited += 1;
                    }
                    ObjectAction::Resize { rect } => {
                        let normalized = rect_normalize(*rect);
                        let width = normalized[2] - normalized[0];
                        let height = normalized[3] - normalized[1];
                        if width < 1.0 || height < 1.0 {
                            return Err(PdfError::InvalidInput("resize rectangle is degenerate".into()));
                        }
                        // Resize re-places the image axis-aligned into the new
                        // rectangle; any previous rotation in the placement is
                        // dropped, which is what "set this rectangle" means.
                        desired = Matrix::translate(normalized[0], normalized[1]).mul(Matrix::scale(width, height));
                        image_intents.insert(key, ImageIntent::Matrix(placement, desired));
                        report.edited += 1;
                    }
                    ObjectAction::Rotate { degrees } => {
                        let bbox = matrix_bbox(desired);
                        let cx = (bbox[0] + bbox[2]) / 2.0;
                        let cy = (bbox[1] + bbox[3]) / 2.0;
                        desired = Matrix::translate(cx, cy)
                            .mul(Matrix::rotate_deg(*degrees))
                            .mul(Matrix::translate(-cx, -cy))
                            .mul(desired);
                        image_intents.insert(key, ImageIntent::Matrix(placement, desired));
                        report.edited += 1;
                    }
                    ObjectAction::Delete => {
                        // A drawn image has no object to remove from a list;
                        // its `/Name Do` sequence is blanked out instead.
                        // Images left in the resources that are no longer
                        // drawn stay in the file (cleaning those up is the
                        // compressor's/sanitizer's job, not this editor's).
                        image_intents.insert(key, ImageIntent::Delete(placement));
                        report.deleted += 1;
                    }
                }
            }
        }
    }

    // Flush stream operations grouped per stream. All offsets anchor on the
    // original decoded stream; applying them from the highest offset down
    // keeps every earlier offset valid even when a patch changes byte length.
    let mut ops_by_stream: HashMap<ObjectId, Vec<(usize, StreamOp)>> = HashMap::new();
    for intent in image_intents.into_values() {
        match intent {
            ImageIntent::Matrix(placement, desired) => {
                let inverse = match placement.matrix.inverse() {
                    Some(inverse) => inverse,
                    None => {
                        report.warnings.push(format!("image '{}' has a degenerate placement matrix", placement.name));
                        continue;
                    }
                };
                match &placement.last_cm {
                    Some((last_cm, ranges)) if ranges.len() == 6 => {
                        // New value for the last `cm`: desired ∘ CTM⁻¹ ∘ M_last.
                        let new_matrix = desired.mul(inverse).mul(*last_cm);
                        ops_by_stream.entry(placement.stream_id).or_default().push((
                            ranges[0].0,
                            StreamOp::Replace {
                                ranges: ranges.clone(),
                                values: new_matrix.0,
                            },
                        ));
                    }
                    _ => {
                        // No single patchable `cm`: insert one before `Do`.
                        let new_matrix = desired.mul(inverse);
                        ops_by_stream.entry(placement.stream_id).or_default().push((
                            placement.do_range.0,
                            StreamOp::Insert {
                                at: placement.do_range.0,
                                values: new_matrix.0,
                            },
                        ));
                    }
                }
            }
            ImageIntent::Delete(placement) => {
                ops_by_stream.entry(placement.stream_id).or_default().push((
                    placement.name_range.0,
                    StreamOp::Blank {
                        start: placement.name_range.0,
                        end: placement.do_range.1,
                    },
                ));
            }
        }
    }
    for (stream_id, mut ops) in ops_by_stream {
        let mut buffer = match decoded_stream_bytes(doc, stream_id) {
            Some(bytes) => bytes,
            None => {
                report.warnings.push(format!("stream {} could not be decoded; image edit skipped", stream_id.0));
                continue;
            }
        };
        ops.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        for (_, op) in ops {
            match op {
                StreamOp::Replace { ranges, values } => match rewrite_matrix_bytes(&buffer, &ranges, values) {
                    Some(next) => buffer = next,
                    None => report.warnings.push("an image matrix could not be patched".into()),
                },
                StreamOp::Insert { at, values } => match insert_matrix_bytes(&buffer, at, values) {
                    Some(next) => buffer = next,
                    None => report.warnings.push("an image matrix could not be inserted".into()),
                },
                StreamOp::Blank { start, end } => {
                    if start < end && end <= buffer.len() {
                        for byte in buffer[start..end].iter_mut() {
                            *byte = b' ';
                        }
                    }
                }
            }
        }
        if let Err(error) = replace_stream_bytes(doc, stream_id, buffer) {
            report.warnings.push(format!("stream {} could not be updated: {error}", stream_id.0));
        }
    }

    // Deletions last: inline annotation positions were snapshot before any
    // mutation, so process them back to front within each page.
    deletions.sort_by(|a, b| {
        let key = |handle: &AnnotHandle| match handle {
            AnnotHandle::Inline { page_id, position } => (*page_id, *position, 0usize),
            AnnotHandle::Ref(id) => (*id, 0usize, 1usize),
        };
        let (page_a, pos_a, kind_a) = key(&a.0);
        let (page_b, pos_b, kind_b) = key(&b.0);
        (page_a, pos_a, kind_a).cmp(&(page_b, pos_b, kind_b))
    });
    deletions.reverse();
    let mut delete_ids: HashSet<ObjectId> = HashSet::new();
    for (handle, ids) in &deletions {
        for id in ids {
            delete_ids.insert(*id);
        }
        remove_annot_handle(doc, *handle)?;
    }
    if !delete_ids.is_empty() {
        remove_field_references(doc, &delete_ids);
        prune_empty_field_nodes(doc);
        for id in &delete_ids {
            doc.objects.remove(id);
        }
        drop_empty_acroform(doc);
    }

    Ok(report)
}

/// Removes the `/AcroForm` entry when no fields are left, matching what the
/// flatten module does after removing widgets.
fn drop_empty_acroform(doc: &mut Document) {
    let catalog_id = match doc.trailer.get(b"Root").and_then(|value| value.as_reference()) {
        Ok(id) => id,
        Err(_) => return,
    };
    let value = match doc.get_dictionary(catalog_id).ok().and_then(|catalog| catalog.get(b"AcroForm").ok().cloned()) {
        Some(value) => value,
        None => return,
    };
    let empty = match &value {
        Object::Reference(id) => doc
            .get_dictionary(*id)
            .ok()
            .map(|acro| resolve_array(doc, acro.get(b"Fields").ok()).map(|fields| fields.is_empty()).unwrap_or(true))
            .unwrap_or(true),
        Object::Dictionary(acro) => resolve_array(doc, acro.get(b"Fields").ok())
            .map(|fields| fields.is_empty())
            .unwrap_or(true),
        _ => return,
    };
    if !empty {
        return;
    }
    if let Object::Reference(id) = value {
        doc.objects.remove(&id);
    }
    if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
        catalog.remove(b"AcroForm");
    }
}

fn validate_action(action: &ObjectAction) -> PdfResult<()> {
    match action {
        ObjectAction::Move { dx, dy } => {
            if !dx.is_finite() || !dy.is_finite() {
                return Err(PdfError::InvalidInput("move delta is not finite".into()));
            }
        }
        ObjectAction::Resize { rect } => {
            if rect.iter().any(|value| !value.is_finite()) {
                return Err(PdfError::InvalidInput("resize rectangle is not finite".into()));
            }
        }
        ObjectAction::Rotate { degrees } => {
            if !degrees.is_finite() {
                return Err(PdfError::InvalidInput("rotation is not finite".into()));
            }
        }
        ObjectAction::Delete => {}
    }
    Ok(())
}

/// Rotates an annotation: the appearance stream's `/Matrix` is rotated about
/// the `/BBox` center, and for quarter turns the `/Rect` is reshaped so the
/// rotated content is not clipped by the old box.
fn rotate_annotation(doc: &mut Document, handle: AnnotHandle, degrees: f64) -> PdfResult<()> {
    let normalized = ((degrees % 360.0) + 360.0) % 360.0;
    if normalized.abs() < 1e-9 {
        return Ok(());
    }
    // Read the live dictionary: previous edits in the same request may have
    // already changed the rect or appearance.
    let dict = annot_dict_snapshot(doc, handle).unwrap_or_default();
    if (normalized - 90.0).abs() < 1e-6 || (normalized - 270.0).abs() < 1e-6 {
        if let Some(rect) = rect_of(doc, &dict) {
            let r = rect_normalize(rect);
            let cx = (r[0] + r[2]) / 2.0;
            let cy = (r[1] + r[3]) / 2.0;
            let width = r[2] - r[0];
            let height = r[3] - r[1];
            let rotated = rect_to_object([cx - height / 2.0, cy - width / 2.0, cx + height / 2.0, cy + width / 2.0]);
            with_annot_dict(doc, handle, |target| target.set("Rect", rotated.clone()))?;
        }
    }
    // Rotate every appearance stream (all states) about its own BBox center.
    let ap = resolve_dict(doc, dict.get(b"AP").ok());
    let normal = ap.and_then(|ap| ap.get(b"N").ok().cloned());
    let stream_ids: Vec<ObjectId> = match normal {
        Some(Object::Reference(id)) => vec![id],
        Some(Object::Dictionary(states)) => states
            .iter()
            .filter_map(|(_, value)| value.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    for stream_id in stream_ids {
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(stream_id) {
            let bbox = stream
                .dict
                .get(b"BBox")
                .ok()
                .and_then(numbers_from_object)
                .filter(|values| values.len() >= 4)
                .map(|values| [values[0], values[1], values[2], values[3]])
                .unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let old = stream
                .dict
                .get(b"Matrix")
                .ok()
                .and_then(matrix_from_object)
                .unwrap_or(Matrix::IDENTITY);
            let cx = (bbox[0] + bbox[2]) / 2.0;
            let cy = (bbox[1] + bbox[3]) / 2.0;
            let rotated = Matrix::translate(cx, cy)
                .mul(Matrix::rotate_deg(normalized))
                .mul(Matrix::translate(-cx, -cy))
                .mul(old);
            stream.dict.set("Matrix", matrix_to_object(rotated));
        }
    }
    Ok(())
}

fn replace_stream_bytes(doc: &mut Document, stream_id: ObjectId, bytes: Vec<u8>) -> PdfResult<()> {
    let stream = doc
        .get_object_mut(stream_id)
        .map_err(PdfError::from)?
        .as_stream_mut()
        .map_err(PdfError::from)?;
    // The patched bytes are plain content: dropping the filters keeps them
    // readable. The writer re-compresses streams on save when asked to.
    stream.dict.remove(b"Filter");
    stream.dict.remove(b"DecodeParms");
    stream.content = bytes;
    Ok(())
}

/// Reads the live dictionary of an annotation entry (indirect or inline).
fn annot_dict_snapshot(doc: &Document, handle: AnnotHandle) -> Option<Dictionary> {
    match handle {
        AnnotHandle::Ref(id) => doc.get_dictionary(id).ok().cloned(),
        AnnotHandle::Inline { page_id, position } => {
            let (_, items) = annots_storage(doc, page_id)?;
            match items.get(position)? {
                Object::Dictionary(dict) => Some(dict.clone()),
                _ => None,
            }
        }
    }
}

/// Applies a closure to an annotation dictionary, indirect or inline.
fn with_annot_dict(
    doc: &mut Document,
    handle: AnnotHandle,
    f: impl FnOnce(&mut Dictionary),
) -> PdfResult<()> {
    match handle {
        AnnotHandle::Ref(id) => with_object_dict(doc, id, f),
        AnnotHandle::Inline { page_id, position } => {
            let storage = annots_storage(doc, page_id).ok_or_else(|| PdfError::CorruptPdf("page has no /Annots".into()))?;
            let mut items = storage.1;
            let mut dict = match items.get(position) {
                Some(Object::Dictionary(dict)) => dict.clone(),
                _ => return Err(PdfError::InvalidInput("inline annotation is no longer present".into())),
            };
            f(&mut dict);
            if let Some(slot) = items.get_mut(position) {
                *slot = Object::Dictionary(dict);
            }
            write_annot_array(doc, page_id, storage.0, items)
        }
    }
}

fn write_annot_array(doc: &mut Document, page_id: ObjectId, storage: AnnotsStorage, items: Vec<Object>) -> PdfResult<()> {
    match storage {
        AnnotsStorage::Inline => {
            let page = doc.get_object_mut(page_id).map_err(PdfError::from)?.as_dict_mut().map_err(PdfError::from)?;
            page.set("Annots", Object::Array(items));
            Ok(())
        }
        AnnotsStorage::Array(id) => {
            *doc.get_object_mut(id).map_err(PdfError::from)?.as_array_mut().map_err(PdfError::from)? = items;
            Ok(())
        }
    }
}

/// Removes an annotation entry from its page's `/Annots` array.
fn remove_annot_handle(doc: &mut Document, handle: AnnotHandle) -> PdfResult<()> {
    match handle {
        AnnotHandle::Ref(id) => {
            let pages = page_map(doc);
            for (_, page_id) in pages {
                if let Some((storage, mut items)) = annots_storage(doc, page_id) {
                    let before = items.len();
                    items.retain(|entry| entry.as_reference().map(|entry_id| entry_id != id).unwrap_or(true));
                    if items.len() != before {
                        write_annot_array(doc, page_id, storage, items)?;
                    }
                }
            }
            Ok(())
        }
        AnnotHandle::Inline { page_id, position } => {
            if let Some((storage, mut items)) = annots_storage(doc, page_id) {
                if position < items.len() && matches!(items.get(position), Some(Object::Dictionary(_))) {
                    items.remove(position);
                    write_annot_array(doc, page_id, storage, items)?;
                }
            }
            Ok(())
        }
    }
}

/// Removes references to deleted widget ids from every `/Fields` and `/Kids`
/// array in the document (the same walk flattening uses).
fn remove_field_references(doc: &mut Document, remove: &HashSet<ObjectId>) {
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

/// Drops field nodes left with an empty `/Kids` array after a widget delete.
fn prune_empty_field_nodes(doc: &mut Document) {
    for _ in 0..4 {
        let mut empty: HashSet<ObjectId> = HashSet::new();
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

/// Byte-level entry point: parse `pdf`, apply `edits`, serialize.
///
/// For password protected inputs, load through `docutil::load_document` and
/// call `apply_object_edits` instead.
pub fn transform_objects(pdf: &[u8], edits: &[ObjectEdit]) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::from_lopdf(error, None))?;
    apply_object_edits(&mut doc, edits)?;
    doc.prune_objects();
    doc.renumber_objects();
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(PdfError::from_io)?;
    Ok(out)
}

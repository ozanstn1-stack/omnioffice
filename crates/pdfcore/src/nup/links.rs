//! Carries `/Link` annotations from the source pages onto their N-up sheets.
//!
//! A sheet page paints its source pages as Form XObjects, and annotations
//! cannot live inside a form: they belong to a page. Links are therefore
//! copied out of the source page's `/Annots`, their rectangles mapped through
//! the placement matrix into the sheet's space (and clipped to the cell they
//! were placed in, because the form's BBox clips the artwork there too), and
//! their targets rewritten to the sheet by [`super::dests`].
//!
//! Every other annotation kind (`/Text`, `/Highlight`, widgets, ...) is left
//! behind deliberately: its appearance stream lives in source-page space and
//! would need per-subtype rewriting that belongs to a flattening pass, not to
//! imposition.

use super::dests::DestMap;
use super::sheet::Placement;
use crate::docutil::Matrix;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// Annotation keys copied verbatim from the source (inlined when indirect):
/// appearance hints that mean the same thing on the sheet.
const COPIED_KEYS: [&str; 7] = ["Border", "BS", "C", "H", "F", "Contents", "NM"];

/// Copies the source pages' link annotations onto the sheets that carry them.
///
/// Never fails: an annotation that cannot be mapped (unreadable dictionary, a
/// destination outside the imposed pages, an unsupported action) is dropped on
/// its own and the imposition goes on.
pub(super) fn carry_links(doc: &mut Document, sheets: &[(ObjectId, Vec<Placement>)], page_order: &[ObjectId]) {
    let dests = DestMap::new(sheets, page_order);
    // Read phase: every copy is built against an immutable document, so the
    // source annotation dictionaries are never touched.
    let mut pending: Vec<(ObjectId, Vec<Dictionary>)> = Vec::with_capacity(sheets.len());
    for (sheet_id, placements) in sheets {
        let mut copies: Vec<Dictionary> = Vec::new();
        for placement in placements {
            for source in source_links(doc, placement.source) {
                if let Some(copy) = copy_link(doc, &source, placement, *sheet_id, &dests) {
                    copies.push(copy);
                }
            }
        }
        pending.push((*sheet_id, copies));
    }
    // Write phase: store the copies and attach them to their sheet. A sheet
    // with no links gets no `/Annots` key at all.
    for (sheet_id, copies) in pending {
        if copies.is_empty() {
            continue;
        }
        let refs: Vec<Object> =
            copies.into_iter().map(|dict| Object::Reference(doc.add_object(Object::Dictionary(dict)))).collect();
        if let Ok(page) = doc.get_dictionary_mut(sheet_id) {
            page.set("Annots", Object::Array(refs));
        }
    }
}

/// The page's `/Link` annotation dictionaries (inline or behind a reference);
/// broken entries are skipped rather than reported.
fn source_links(doc: &Document, page_id: ObjectId) -> Vec<Dictionary> {
    let Ok(page) = doc.get_dictionary(page_id) else { return Vec::new() };
    let Ok(annots) = page.get(b"Annots") else { return Vec::new() };
    let items: Vec<Object> = match annots {
        Object::Array(items) => items.clone(),
        Object::Reference(id) => {
            doc.get_object(*id).ok().and_then(|object| object.as_array().ok().cloned()).unwrap_or_default()
        }
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let dict = match item {
                Object::Reference(id) => doc.get_dictionary(*id).ok()?,
                Object::Dictionary(dict) => dict,
                _ => return None,
            };
            let subtype = dict.get(b"Subtype").ok()?.as_name().ok()?;
            (subtype == b"Link").then(|| dict.clone())
        })
        .collect()
}

fn copy_link(
    doc: &Document,
    source: &Dictionary,
    placement: &Placement,
    sheet_id: ObjectId,
    dests: &DestMap,
) -> Option<Dictionary> {
    let rect = read_rect(doc, source.get(b"Rect").ok()?)?;
    let rect = clip_rect(map_rect(&placement.matrix, rect), placement.clip)?;
    let target = if let Ok(action) = source.get(b"A") {
        // A link carries either an action or a destination, never both; an
        // action wins, and an unsupported one drops the link instead of
        // silently retargeting it.
        dests.action(doc, action)?
    } else {
        super::dests::LinkTarget::Internal(dests.destination(doc, source.get(b"Dest").ok()?)?)
    };
    let mut copy = Dictionary::new();
    copy.set("Type", Object::Name(b"Annot".to_vec()));
    copy.set("Subtype", Object::Name(b"Link".to_vec()));
    copy.set("Rect", rect_array(rect));
    copy.set("P", Object::Reference(sheet_id));
    for key in COPIED_KEYS {
        if let Ok(value) = source.get(key.as_bytes()) {
            copy.set(key, inline(doc, value));
        }
    }
    if let Some(quad) = source.get(b"QuadPoints").ok().and_then(|value| quad_points(doc, &placement.matrix, value)) {
        copy.set("QuadPoints", Object::Array(quad));
    }
    match target {
        super::dests::LinkTarget::Uri(action) => copy.set("A", Object::Dictionary(action)),
        super::dests::LinkTarget::Internal(dest) => copy.set("Dest", dest),
    }
    Some(copy)
}

/// One indirect hop to an inline value; a dangling reference keeps the
/// reference (the value is unreadable either way and a viewer ignores it).
fn inline(doc: &Document, value: &Object) -> Object {
    match value {
        Object::Reference(id) => doc.get_object(*id).cloned().unwrap_or_else(|_| value.clone()),
        other => other.clone(),
    }
}

fn read_rect(doc: &Document, value: &Object) -> Option<[f64; 4]> {
    let items = inline(doc, value);
    let items = items.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, item) in items.iter().enumerate() {
        out[slot] = number(item)?;
    }
    // Some producers write the corners in reverse order.
    Some([out[0].min(out[2]), out[1].min(out[3]), out[0].max(out[2]), out[1].max(out[3])])
}

/// `/QuadPoints` in groups of four corner points; unusable values (not a
/// multiple of eight, non-numeric) are dropped, the rectangle is enough.
fn quad_points(doc: &Document, matrix: &Matrix, value: &Object) -> Option<Vec<Object>> {
    let items = inline(doc, value);
    let items = items.as_array().ok()?;
    if items.is_empty() || !items.len().is_multiple_of(8) {
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    let mut index = 0;
    while index < items.len() {
        let (x, y) = (number(&items[index])?, number(&items[index + 1])?);
        let (x, y) = matrix.apply(x, y);
        out.push(Object::Real(x as f32));
        out.push(Object::Real(y as f32));
        index += 2;
    }
    Some(out)
}

/// Maps a page-space rectangle through the placement matrix. The source page
/// rotation is inside that matrix, so the result is the axis-aligned box the
/// link covers on the sheet.
pub(super) fn map_rect(matrix: &Matrix, rect: [f64; 4]) -> [f64; 4] {
    let corners = [(rect[0], rect[1]), (rect[2], rect[1]), (rect[0], rect[3]), (rect[2], rect[3])];
    let mapped = corners.map(|(x, y)| matrix.apply(x, y));
    let min_x = mapped.iter().map(|point| point.0).fold(f64::MAX, f64::min);
    let max_x = mapped.iter().map(|point| point.0).fold(f64::MIN, f64::max);
    let min_y = mapped.iter().map(|point| point.1).fold(f64::MAX, f64::min);
    let max_y = mapped.iter().map(|point| point.1).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x, max_y]
}

/// Intersects a rectangle with the cell a page was placed in; `None` when
/// nothing visible is left (a link entirely outside the artwork).
pub(super) fn clip_rect(rect: [f64; 4], clip: [f64; 4]) -> Option<[f64; 4]> {
    let out = [rect[0].max(clip[0]), rect[1].max(clip[1]), rect[2].min(clip[2]), rect[3].min(clip[3])];
    (out[2] - out[0] >= 0.01 && out[3] - out[1] >= 0.01).then_some(out)
}

pub(super) fn rect_array(rect: [f64; 4]) -> Object {
    Object::Array(rect.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn number(value: &Object) -> Option<f64> {
    let number = match value {
        Object::Integer(value) => *value as f64,
        Object::Real(value) => *value as f64,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

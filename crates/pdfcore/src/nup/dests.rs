//! Destination rewriting for imposed sheets.
//!
//! A link inside an N-up PDF pointed at a page of the *source* document. The
//! imposition replaces that tree, so every internal destination is rewritten
//! to name the sheet page the source page ended up on, with its view reduced
//! to the sheet-space rectangle of the cell (`/FitR`) unless the original view
//! was an exact point (`/XYZ`), which maps through the placement matrix.
//!
//! Named destinations (`/Names << /Dests ... >>` trees and the older catalog
//! `/Dests` dictionary) are resolved here and written out as explicit arrays,
//! so the output keeps no dependency on the source document's name tree.

use super::links::{clip_rect, map_rect, rect_array};
use super::sheet::Placement;
use crate::docutil::Matrix;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{HashMap, HashSet};

/// Depth guard for reference chains (`/Dest` pointing at `/D` arrays, name
/// trees nested through kids): deeper than any real document uses.
const MAX_DEPTH: usize = 32;

/// Where one source page ended up on the sheets.
#[derive(Debug, Clone, Copy)]
struct Target {
    sheet: ObjectId,
    matrix: Matrix,
    clip: [f64; 4],
}

/// Resolves source destinations against the imposed page tree.
pub(super) struct DestMap {
    by_page: HashMap<ObjectId, Target>,
    /// 1-based page number -> source page object, for integer destinations
    /// (which are zero-based where they appear).
    page_order: Vec<ObjectId>,
}

impl DestMap {
    pub(super) fn new(sheets: &[(ObjectId, Vec<Placement>)], page_order: &[ObjectId]) -> Self {
        let mut by_page: HashMap<ObjectId, Target> = HashMap::new();
        for (sheet_id, placements) in sheets {
            for placement in placements {
                // A page listed twice keeps its first placement; the later one
                // is where the reader is least likely to have come from.
                by_page.entry(placement.source).or_insert(Target {
                    sheet: *sheet_id,
                    matrix: placement.matrix,
                    clip: placement.clip,
                });
            }
        }
        DestMap { by_page, page_order: page_order.to_vec() }
    }

    /// The destination array for a source `/Dest` value, or `None` when it
    /// points outside the imposed pages or cannot be read.
    pub(super) fn destination(&self, doc: &Document, value: &Object) -> Option<Object> {
        let (target, view) = self.resolve(doc, value, 0)?;
        Some(view_object(target, &view))
    }

    fn resolve(&self, doc: &Document, value: &Object, depth: usize) -> Option<(Target, Vec<Object>)> {
        if depth > MAX_DEPTH {
            return None;
        }
        match value {
            Object::Reference(id) => self.resolve(doc, doc.get_object(*id).ok()?, depth + 1),
            Object::Array(items) => {
                let target = self.target_of(items.first()?)?;
                Some((target, items[1..].to_vec()))
            }
            Object::Dictionary(dict) => self.resolve(doc, dict.get(b"D").ok()?, depth + 1),
            Object::String(key, _) | Object::Name(key) => {
                let found = self.named(doc, key)?;
                self.resolve(doc, &found, depth + 1)
            }
            _ => None,
        }
    }

    fn target_of(&self, page: &Object) -> Option<Target> {
        let page_id = match page {
            Object::Reference(id) => *id,
            // Some writers store a page index instead of a reference; readers
            // treat those arrays as zero-based.
            Object::Integer(index) => {
                let slot = usize::try_from(*index).ok()?;
                *self.page_order.get(slot)?
            }
            _ => return None,
        };
        self.by_page.get(&page_id).copied()
    }

    /// Looks a name up in the catalog's `/Names /Dests` tree, then in the older
    /// `/Dests` dictionary.
    fn named(&self, doc: &Document, key: &[u8]) -> Option<Object> {
        let catalog = doc.catalog().ok()?;
        if let Ok(names) = catalog.get(b"Names") {
            let names = resolve(doc, names);
            if let Some(dests) = names.as_dict().ok().and_then(|dict| dict.get(b"Dests").ok()) {
                let mut visited = HashSet::new();
                if let Some(found) = walk_names(doc, resolve(doc, dests), key, 0, &mut visited) {
                    return Some(found);
                }
            }
        }
        let dests = catalog.get(b"Dests").ok()?;
        let dict = resolve(doc, dests).as_dict().ok()?;
        dict.get(key).ok().cloned()
    }
}

/// Follows one indirect reference; anything else is returned as is.
pub(super) fn resolve<'a>(doc: &'a Document, value: &'a Object) -> &'a Object {
    match value {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(value),
        other => other,
    }
}

/// Searches a name tree for `key`, ignoring `/Limits` (linear scan is simpler
/// and tolerates trees written with stale limits).
fn walk_names(
    doc: &Document,
    node: &Object,
    key: &[u8],
    depth: usize,
    visited: &mut HashSet<ObjectId>,
) -> Option<Object> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Object::Reference(id) = node {
        if !visited.insert(*id) {
            return None;
        }
    }
    let dict = resolve(doc, node).as_dict().ok()?;
    if let Ok(names) = dict.get(b"Names") {
        // Both the pair array and the kids array may be indirect.
        let pairs = resolve(doc, names).as_array().ok()?;
        for pair in pairs.chunks(2) {
            if pair.len() != 2 {
                continue;
            }
            let name = match &pair[0] {
                Object::String(bytes, _) | Object::Name(bytes) => bytes.as_slice(),
                _ => continue,
            };
            if name == key {
                return Some(pair[1].clone());
            }
        }
    }
    let kids = resolve(doc, dict.get(b"Kids").ok()?).as_array().ok()?;
    kids.iter().find_map(|kid| walk_names(doc, kid, key, depth + 1, visited))
}

/// Builds the final destination array: always `[sheet /FitR ...]` or
/// `[sheet /XYZ x y null]`, so the output carries no other view shapes.
fn view_object(target: Target, view: &[Object]) -> Object {
    let fit = |rect: [f64; 4]| fit_array(target.sheet, rect);
    let kind = match view.first() {
        Some(Object::Name(kind)) => Some(kind.clone()),
        _ => None,
    };
    match kind.as_deref() {
        Some(b"XYZ") => {
            let left = view.get(1).and_then(number);
            let top = view.get(2).and_then(number);
            match (left, top) {
                // A point maps exactly; the zoom is dropped so the sheet keeps
                // the reader's setting instead of a source-page magnification.
                (Some(left), Some(top)) => {
                    let (x, y) = target.matrix.apply(left, top);
                    Object::Array(vec![
                        Object::Reference(target.sheet),
                        Object::Name(b"XYZ".to_vec()),
                        Object::Real(x as f32),
                        Object::Real(y as f32),
                        Object::Null,
                    ])
                }
                _ => fit_object(target),
            }
        }
        Some(b"FitR") => {
            let rect = view.get(1..5).and_then(read_rect_items);
            match rect.map(|rect| clip_rect(map_rect(&target.matrix, rect), target.clip)) {
                Some(Some(rect)) => fit(rect),
                _ => fit_object(target),
            }
        }
        // `/Fit`, `/FitB`, `/FitH`, `/FitV`, `/FitBH`, `/FitBV` and anything
        // unknown mean "show me this page"; on a sheet that is the cell.
        _ => fit_object(target),
    }
}

fn fit_object(target: Target) -> Object {
    fit_array(target.sheet, target.clip)
}

/// `[sheet /FitR left bottom right top]`; the rectangle numbers are spread
/// into the array, not nested.
fn fit_array(sheet: ObjectId, rect: [f64; 4]) -> Object {
    let mut items = vec![Object::Reference(sheet), Object::Name(b"FitR".to_vec())];
    if let Object::Array(numbers) = rect_array(rect) {
        items.extend(numbers);
    }
    Object::Array(items)
}

fn read_rect_items(items: &[Object]) -> Option<[f64; 4]> {
    let mut out = [0.0; 4];
    for (slot, item) in items.iter().enumerate() {
        out[slot] = number(item)?;
    }
    Some(out)
}

fn number(value: &Object) -> Option<f64> {
    let number = match value {
        Object::Integer(value) => *value as f64,
        Object::Real(value) => *value as f64,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

/// A destination the sheet can carry: an external URI action or an internal
/// destination rewritten to the sheet.
pub(super) enum LinkTarget {
    Uri(Dictionary),
    Internal(Object),
}

impl DestMap {
    pub(super) fn action(&self, doc: &Document, action: &Object) -> Option<LinkTarget> {
        let dict = resolve(doc, action).as_dict().ok()?;
        let kind = dict.get(b"S").ok()?.as_name().ok()?;
        match kind {
            b"URI" => {
                let uri = dict.get(b"URI").ok()?;
                let mut out = Dictionary::new();
                out.set("S", Object::Name(b"URI".to_vec()));
                out.set("URI", uri.clone());
                if let Ok(is_map) = dict.get(b"IsMap") {
                    out.set("IsMap", is_map.clone());
                }
                Some(LinkTarget::Uri(out))
            }
            b"GoTo" => {
                let dest = dict.get(b"D").ok()?;
                self.destination(doc, dest).map(LinkTarget::Internal)
            }
            // `/GoToR`, `/GoToE`, `/Launch`, `/JavaScript`, `/Named` and the
            // rest point at another file, a script, or a navigation state the
            // sheet no longer has: dropped on purpose.
            _ => None,
        }
    }
}

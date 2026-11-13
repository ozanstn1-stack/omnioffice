//! Cleanup of orphaned PDF annotations on full-rewrite saves (E6.7).
//!
//! When a markup comment is deleted without cleaning up its `/Popup`, `/IRT`
//! reply chain, or unreferenced `/Type /Annot` object, viewers can either keep
//! showing disconnected popups/replies or bloat the file with unreachable
//! annotation dictionaries and appearance streams. This pass sweeps those
//! orphans cleanly before `Document::prune_objects()`, while leaving
//! `/Subtype /Widget` AcroForm parent links intact and never running inside
//! append-only signed revisions.

use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{BTreeSet, HashSet};

fn ref_id(dict: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    dict.get(key).ok().and_then(|value| value.as_reference().ok())
}

fn is_popup(dict: &Dictionary) -> bool {
    dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) == Some(b"Popup")
}

fn is_orphan_dict(dict: &Dictionary, live_annots: &BTreeSet<ObjectId>) -> bool {
    // Only `/Popup` uses `/Parent` to point at another annotation; `/Widget`
    // annotations use `/Parent` for their owning AcroForm field dictionary.
    if is_popup(dict) && ref_id(dict, b"Parent").is_some_and(|target| !live_annots.contains(&target)) {
        return true;
    }
    ref_id(dict, b"IRT").is_some_and(|target| !live_annots.contains(&target))
}

fn collect_acroform_ids(doc: &Document) -> HashSet<ObjectId> {
    let mut visited = HashSet::new();
    let Ok(catalog) = doc.catalog() else { return visited };
    let Ok(acro_obj) = catalog.get(b"AcroForm") else { return visited };
    let mut stack = Vec::new();
    if let Ok(acro_id) = acro_obj.as_reference() {
        stack.push(acro_id);
    } else if let Ok(dict) = acro_obj.as_dict() {
        if let Ok(fields) = dict.get(b"Fields").and_then(|v| doc.dereference(v)).map(|(_, o)| o) {
            if let Ok(arr) = fields.as_array() {
                stack.extend(arr.iter().filter_map(|item| item.as_reference().ok()));
            }
        }
    }
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Ok(dict) = doc.get_dictionary(id) else { continue };
        for key in [b"Fields".as_slice(), b"Kids".as_slice()] {
            if let Ok(arr) = dict.get(key).and_then(|v| doc.dereference(v)).and_then(|(_, o)| o.as_array()) {
                stack.extend(arr.iter().filter_map(|item| item.as_reference().ok()));
            }
        }
    }
    visited
}

struct PageAnnots {
    page_id: ObjectId,
    indirect_array: Option<ObjectId>,
    items: Vec<Object>,
    changed: bool,
}

/// Prunes orphaned popup/reply entries from every page's `/Annots` array,
/// clears stale `/Popup` pointers on surviving annotations, and removes
/// unreferenced `/Type /Annot` dictionaries from `doc.objects`. Returns the
/// number of orphaned annotations removed.
pub fn prune_orphan_annotations(doc: &mut Document) -> u32 {
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut pages = Vec::new();
    let mut live_annots = BTreeSet::new();

    for page_id in page_ids {
        let Ok(page_dict) = doc.get_dictionary(page_id) else { continue };
        let Ok(annots_obj) = page_dict.get(b"Annots").cloned() else { continue };
        let (indirect_array, items) = match annots_obj {
            Object::Array(items) => (None, items),
            Object::Reference(arr_id) => match doc.get_object(arr_id).and_then(|obj| obj.as_array()) {
                Ok(items) => (Some(arr_id), items.clone()),
                Err(_) => continue,
            },
            _ => continue,
        };
        for item in &items {
            if let Ok(id) = item.as_reference() {
                if doc.get_dictionary(id).is_ok() {
                    live_annots.insert(id);
                }
            }
        }
        pages.push(PageAnnots { page_id, indirect_array, items, changed: false });
    }

    let mut dropped_indirect = BTreeSet::new();
    let mut dropped_inline = 0u32;

    // 1. Fixed-point orphan sweep inside page `/Annots` arrays (popups whose
    //    `/Parent` vanished, replies whose `/IRT` vanished, or dangling refs).
    loop {
        let before = live_annots.len();
        for page in &mut pages {
            let mut kept = Vec::with_capacity(page.items.len());
            for item in std::mem::take(&mut page.items) {
                match &item {
                    Object::Reference(id) => {
                        if !live_annots.contains(id) {
                            page.changed = true;
                            continue;
                        }
                        let orphan =
                            doc.get_dictionary(*id).map(|dict| is_orphan_dict(dict, &live_annots)).unwrap_or(true);
                        if orphan {
                            live_annots.remove(id);
                            dropped_indirect.insert(*id);
                            page.changed = true;
                        } else {
                            kept.push(item);
                        }
                    }
                    Object::Dictionary(dict) => {
                        if is_orphan_dict(dict, &live_annots) {
                            dropped_inline += 1;
                            page.changed = true;
                        } else {
                            kept.push(item);
                        }
                    }
                    _ => kept.push(item),
                }
            }
            page.items = kept;
        }
        if live_annots.len() == before {
            break;
        }
    }

    // 2. Strip stale `/Popup` references from surviving inline & indirect annots
    //    so `prune_objects()` does not keep deleted popups alive via back-links.
    for page in &mut pages {
        for item in &mut page.items {
            if let Object::Dictionary(dict) = item {
                if ref_id(dict, b"Popup").is_some_and(|popup| !live_annots.contains(&popup)) {
                    dict.remove(b"Popup");
                    page.changed = true;
                }
            }
        }
        if page.changed {
            if page.items.is_empty() {
                if let Ok(page_dict) = doc.get_dictionary_mut(page.page_id) {
                    page_dict.remove(b"Annots");
                }
            } else if let Some(arr_id) = page.indirect_array {
                if let Ok(obj) = doc.get_object_mut(arr_id) {
                    *obj = Object::Array(std::mem::take(&mut page.items));
                }
            } else if let Ok(page_dict) = doc.get_dictionary_mut(page.page_id) {
                page_dict.set("Annots", Object::Array(std::mem::take(&mut page.items)));
            }
        }
    }

    for &id in &live_annots {
        let stale = doc
            .get_dictionary(id)
            .ok()
            .and_then(|dict| ref_id(dict, b"Popup"))
            .is_some_and(|popup| !live_annots.contains(&popup));
        if stale {
            if let Ok(dict) = doc.get_dictionary_mut(id) {
                dict.remove(b"Popup");
            }
        }
    }

    // 3. Remove any unreferenced `/Type /Annot` dictionaries (both orphans just
    //    dropped from `/Annots` and pre-existing unattached annotation objects)
    //    that are not part of the AcroForm tree.
    let acroform_ids = collect_acroform_ids(doc);
    let mut dead_objects: BTreeSet<ObjectId> = dropped_indirect;
    for (&id, object) in &doc.objects {
        if live_annots.contains(&id) || acroform_ids.contains(&id) {
            continue;
        }
        if let Ok(dict) = object.as_dict() {
            if dict.has_type(b"Annot") {
                dead_objects.insert(id);
            }
        }
    }
    for &id in &dead_objects {
        doc.objects.remove(&id);
    }

    dead_objects.len() as u32 + dropped_inline
}

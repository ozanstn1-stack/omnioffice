//! Removing the references that point at a page somebody just deleted.
//!
//! Dropping a page from the page tree is only half the job: every `/Dest` that
//! named it (ISO 32000-1 12.3.2.2), every outline entry, the catalog's
//! `/OpenAction` and the `/Names /Dests` tree still point at an object that is no
//! longer a page. A reader then offers links that open onto nothing - and the
//! reference keeps the page object alive, so the content of a page the user
//! deleted is still in the file.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::inspect::links::destination_page;

/// What the prune removed, for the caller to report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DestPruneReport {
    pub links_removed: u32,
    pub outline_targets_cleared: u32,
    pub names_removed: u32,
    pub open_action_removed: bool,
}

impl DestPruneReport {
    /// True when nothing pointed at the deleted pages.
    pub fn is_clean(&self) -> bool {
        self.links_removed == 0
            && self.outline_targets_cleared == 0
            && self.names_removed == 0
            && !self.open_action_removed
    }
}

/// Removes every reference to `deleted`, the ids of pages no longer in the tree.
pub fn prune_references_to(doc: &mut Document, deleted: &HashSet<ObjectId>) -> DestPruneReport {
    let mut report = DestPruneReport::default();
    if deleted.is_empty() {
        return report;
    }
    report.links_removed = prune_links(doc, deleted);
    report.outline_targets_cleared = prune_outline(doc, deleted);
    report.names_removed = prune_names(doc, deleted);
    report.open_action_removed = prune_open_action(doc, deleted);
    report
}

/// The destination a dictionary carries: its own `/Dest` or its action's `/D`.
/// `/A` wins when a producer writes both (ISO 32000-1 12.6.2).
fn destination_of(doc: &Document, dict: &Dictionary) -> Option<Object> {
    if let Ok(action) = dict.get(b"A") {
        let action = match action {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        };
        return action.and_then(|action| action.get(b"D").ok().cloned());
    }
    dict.get(b"Dest").ok().cloned()
}

fn targets_deleted(doc: &Document, dest: &Object, deleted: &HashSet<ObjectId>) -> bool {
    // A name is resolved through the document's own tree first; the entry is a
    // `/Dest` in its own right (ISO 32000-1 12.3.2.3).
    if matches!(dest, Object::Name(_) | Object::String(_, _)) {
        if let Some(resolved) = name_destination(doc, dest) {
            return targets_deleted(doc, &resolved, deleted);
        }
    }
    // The page reference inside the destination, which is what is left once the
    // page has already been dropped from the tree: `destination_page` cannot find
    // a page number for an object that is not a page any more.
    if let Some(id) = destination_page_ref(doc, dest) {
        return deleted.contains(&id);
    }
    match destination_page(doc, dest) {
        Some(page) => doc.get_pages().get(&page).map(|id| deleted.contains(id)).unwrap_or(false),
        None => matches!(dest, Object::Reference(id) if deleted.contains(id)),
    }
}

/// The page object a destination names, whether or not it is still a page.
fn destination_page_ref(doc: &Document, dest: &Object) -> Option<ObjectId> {
    let array = match dest {
        Object::Array(items) => items.clone(),
        Object::Dictionary(entry) => match entry.get(b"D").ok() {
            Some(Object::Array(items)) => items.clone(),
            _ => return None,
        },
        Object::Reference(id) => return Some(*id),
        _ => return None,
    };
    let _ = doc;
    [0usize, 1].into_iter().find_map(|index| array.get(index)?.as_reference().ok())
}

fn prune_links(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    let mut removed = 0;
    for page_id in doc.get_pages().values().copied().collect::<Vec<_>>() {
        let annots = match doc.get_dictionary(page_id).and_then(|page| page.get(b"Annots")) {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(Object::as_array) {
                Ok(items) => items.clone(),
                Err(_) => continue,
            },
            _ => continue,
        };
        let mut kept: Vec<Object> = Vec::with_capacity(annots.len());
        let mut changed = false;
        for item in annots {
            let dead = item
                .as_reference()
                .ok()
                .and_then(|id| doc.get_dictionary(id).ok())
                .filter(|annot| annot.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"") == b"Link")
                .and_then(|annot| destination_of(doc, annot))
                .map(|dest| targets_deleted(doc, &dest, deleted))
                .unwrap_or(false);
            if dead {
                removed += 1;
                changed = true;
            } else {
                kept.push(item);
            }
        }
        if changed {
            if let Ok(page) = doc.get_object_mut(page_id).and_then(Object::as_dict_mut) {
                if kept.is_empty() {
                    page.remove(b"Annots");
                } else {
                    page.set("Annots", Object::Array(kept));
                }
            }
        }
    }
    removed
}

fn prune_outline(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"Outlines").ok().cloned())
        .and_then(|value| value.as_reference().ok())
    else {
        return 0;
    };
    let mut cleared = 0;
    let mut current = doc.get_dictionary(root).ok().and_then(|dict| dict.get(b"First").ok().cloned());
    let mut hops = 0;
    while let Some(value) = current {
        hops += 1;
        if hops > 10_000 {
            break;
        }
        let Some(id) = value.as_reference().ok() else { break };
        let Ok(entry) = doc.get_dictionary(id) else { break };
        let next = entry.get(b"Next").ok().cloned();
        let entry = entry.clone();
        let dead = destination_of(doc, &entry).map(|dest| targets_deleted(doc, &dest, deleted)).unwrap_or(false);
        if dead {
            if let Ok(dict) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                dict.remove(b"Dest");
                dict.remove(b"A");
                cleared += 1;
            }
        }
        current = next;
    }
    cleared
}

fn prune_names(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    let Some(tree) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"Names").ok().cloned())
        .and_then(|value| match value {
            Object::Reference(id) => doc.get_dictionary(id).ok().cloned(),
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .and_then(|names| names.get(b"Dests").ok().cloned())
    else {
        return 0;
    };
    let mut seen = HashSet::new();
    prune_name_node(doc, &tree, deleted, &mut seen)
}

fn prune_name_node(
    doc: &mut Document,
    node: &Object,
    deleted: &HashSet<ObjectId>,
    seen: &mut HashSet<ObjectId>,
) -> u32 {
    let id = match node {
        Object::Reference(id) => {
            if !seen.insert(*id) {
                return 0;
            }
            Some(*id)
        }
        Object::Dictionary(_) => None,
        _ => return 0,
    };
    let dict = match id {
        Some(id) => doc.get_dictionary(id).ok().cloned(),
        None => match node {
            Object::Dictionary(dict) => Some(dict.clone()),
            _ => None,
        },
    };
    let Some(dict) = dict else { return 0 };
    let mut removed = 0;
    if let Ok(pairs) = dict.get(b"Names").and_then(Object::as_array) {
        let mut kept: Vec<Object> = Vec::with_capacity(pairs.len());
        let mut changed = false;
        for pair in pairs.chunks(2) {
            let [name, value] = pair else { continue };
            let target = match value {
                Object::Dictionary(entry) => entry.get(b"D").ok().cloned(),
                other => Some(other.clone()),
            };
            if target.map(|target| targets_deleted(doc, &target, deleted)).unwrap_or(false) {
                removed += 1;
                changed = true;
            } else {
                kept.push(name.clone());
                kept.push(value.clone());
            }
        }
        if changed {
            if let (Some(id), Ok(entry)) = (id, doc.get_object_mut(id.unwrap_or((0, 0))).map_err(|_| ())) {
                let _ = entry;
                if let Ok(dict_mut) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                    dict_mut.set("Names", Object::Array(kept));
                }
            }
        }
    }
    for kid in dict.get(b"Kids").and_then(Object::as_array).cloned().unwrap_or_default() {
        removed += prune_name_node(doc, &kid, deleted, seen);
    }
    removed
}

fn prune_open_action(doc: &mut Document, deleted: &HashSet<ObjectId>) -> bool {
    let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) else { return false };
    let action = doc.get_dictionary(root).ok().and_then(|catalog| catalog.get(b"OpenAction").ok().cloned());
    let Some(action) = action else { return false };
    let dest = match &action {
        Object::Dictionary(dict) => dict.get(b"D").ok().cloned().unwrap_or_else(|| action.clone()),
        other => other.clone(),
    };
    if !targets_deleted(doc, &dest, deleted) {
        return false;
    }
    if let Ok(dict) = doc.get_object_mut(root).and_then(Object::as_dict_mut) {
        dict.remove(b"OpenAction");
        return true;
    }
    false
}

/// The destination a name stands for, from the document's `/Names /Dests` tree.
fn name_destination(doc: &Document, key: &Object) -> Option<Object> {
    let tree = doc.catalog().ok()?.get(b"Names").ok()?.clone();
    let tree = match tree {
        Object::Reference(id) => doc.get_dictionary(id).ok()?.get(b"Dests").ok()?.clone(),
        Object::Dictionary(dict) => dict.get(b"Dests").ok()?.clone(),
        _ => return None,
    };
    let mut seen = HashSet::new();
    find_in_name_node(doc, &tree, key, &mut seen)
}

fn find_in_name_node(doc: &Document, node: &Object, key: &Object, seen: &mut HashSet<ObjectId>) -> Option<Object> {
    let id = match node {
        Object::Reference(id) => {
            if !seen.insert(*id) {
                return None;
            }
            Some(*id)
        }
        Object::Dictionary(_) => None,
        _ => return None,
    };
    let dict = match id {
        Some(id) => doc.get_dictionary(id).ok()?.clone(),
        None => match node {
            Object::Dictionary(dict) => dict.clone(),
            _ => return None,
        },
    };
    if let Ok(pairs) = dict.get(b"Names").and_then(Object::as_array) {
        for pair in pairs.chunks(2) {
            let [name, value] = pair else { continue };
            if !names_match(name, key) {
                continue;
            }
            return match value {
                Object::Dictionary(entry) => entry.get(b"D").ok().cloned(),
                other => Some(other.clone()),
            };
        }
    }
    for kid in dict.get(b"Kids").and_then(Object::as_array).cloned().unwrap_or_default() {
        if let Some(found) = find_in_name_node(doc, &kid, key, seen) {
            return Some(found);
        }
    }
    None
}

fn names_match(name: &Object, key: &Object) -> bool {
    let left = match name {
        Object::String(bytes, _) => Some(bytes.clone()),
        Object::Name(bytes) => Some(bytes.clone()),
        _ => None,
    };
    let right = match key {
        Object::String(bytes, _) => Some(bytes.clone()),
        Object::Name(bytes) => Some(bytes.clone()),
        _ => None,
    };
    left.is_some() && left == right
}

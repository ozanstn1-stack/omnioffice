//! Actions and structure references that still point at a page the plan dropped.
//!
//! An action can carry a chain of steps (`/Next`, ISO 32000-1 12.6.2) and an
//! annotation, a field or the catalog can hang additional actions on events
//! (`/AA`, 12.6.3). An outline item can name the structure element it stands for
//! (`/SE`, 12.3.3). Each of them can still reach the page that is gone - and a
//! reference keeps the page object, and the content on it, alive.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::dest_prune::targets_deleted_page;
use crate::inspect::links::destination_page;

/// Drops the actions that would jump to a deleted page, and prunes `/Next` chains.
pub fn prune_actions(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    if deleted.is_empty() {
        return 0;
    }
    let mut removed = 0;
    // The catalog's own action and its additional-action dictionary.
    if let Ok(root) = doc.trailer.get(b"Root").and_then(Object::as_reference) {
        removed += prune_dict_actions(doc, root, deleted);
    }
    // Every page's annotations, and the fields of the form.
    for page_id in doc.get_pages().values().copied().collect::<Vec<_>>() {
        let annots = match doc.get_dictionary(page_id).and_then(|page| page.get(b"Annots")) {
            Ok(Object::Array(items)) => items.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(Object::as_array) {
                Ok(items) => items.clone(),
                Err(_) => continue,
            },
            _ => continue,
        };
        for item in annots {
            if let Ok(id) = item.as_reference() {
                removed += prune_dict_actions(doc, id, deleted);
            }
        }
    }
    for field in form_fields(doc) {
        removed += prune_dict_actions(doc, field, deleted);
    }
    removed += prune_outline_references(doc, deleted);
    removed
}

fn form_fields(doc: &Document) -> Vec<ObjectId> {
    let Some(form) = doc.catalog().ok().and_then(|catalog| catalog.get(b"AcroForm").ok().cloned()) else {
        return Vec::new();
    };
    let dict = match form {
        Object::Reference(id) => match doc.get_dictionary(id) {
            Ok(dict) => dict.clone(),
            Err(_) => return Vec::new(),
        },
        Object::Dictionary(dict) => dict,
        _ => return Vec::new(),
    };
    dict.get(b"Fields")
        .and_then(Object::as_array)
        .map(|items| items.iter().filter_map(|item| item.as_reference().ok()).collect())
        .unwrap_or_default()
}

/// Prunes `/AA` and `/OpenAction` on one dictionary, step by step.
fn prune_dict_actions(doc: &mut Document, id: ObjectId, deleted: &HashSet<ObjectId>) -> u32 {
    let Ok(dict) = doc.get_dictionary(id).cloned() else { return 0 };
    let mut removed = 0;

    // Additional actions: each event keeps its entry unless the step itself is dead.
    if let Ok(aa) = dict.get(b"AA").cloned() {
        let (aa_id, aa_dict) = match &aa {
            Object::Reference(aa_id) => match doc.get_dictionary(*aa_id) {
                Ok(entry) => (Some(*aa_id), entry.clone()),
                Err(_) => (None, Default::default()),
            },
            Object::Dictionary(entry) => (None, entry.clone()),
            _ => (None, Default::default()),
        };
        let mut updates: Vec<(Vec<u8>, Option<Object>)> = Vec::new();
        for (key, value) in aa_dict.iter() {
            match prune_action_value(doc, value, deleted) {
                Some(pruned) if &pruned != value => updates.push((key.to_vec(), Some(pruned))),
                Some(_) => {}
                None => updates.push((key.to_vec(), None)),
            }
        }
        if !updates.is_empty() {
            for (key, value) in updates {
                removed += 1;
                match (aa_id, value) {
                    (Some(aa_id), Some(value)) => {
                        if let Ok(entry) = doc.get_object_mut(aa_id).and_then(Object::as_dict_mut) {
                            entry.set(key, value);
                        }
                    }
                    (Some(aa_id), None) => {
                        if let Ok(entry) = doc.get_object_mut(aa_id).and_then(Object::as_dict_mut) {
                            entry.remove(key.as_slice());
                        }
                    }
                    (None, value) => {
                        if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                            if let Ok(Object::Dictionary(aa)) = entry.get_mut(b"AA") {
                                match value {
                                    Some(value) => aa.set(key, value),
                                    None => {
                                        aa.remove(key.as_slice());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // An empty action dictionary is noise.
            if let Some(aa_id) = aa_id {
                let empty = doc.get_dictionary(aa_id).map(|entry| entry.is_empty()).unwrap_or(false);
                if empty {
                    if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                        entry.remove(b"AA");
                    }
                }
            }
        }
    }

    // The catalog's open action is one chain, pruned step by step.
    if let Ok(open_action) = dict.get(b"OpenAction").cloned() {
        match prune_action_value(doc, &open_action, deleted) {
            None => {
                if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                    entry.remove(b"OpenAction");
                    removed += 1;
                }
            }
            Some(pruned) if pruned != open_action => {
                if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                    entry.set("OpenAction", pruned);
                    removed += 1;
                }
            }
            Some(_) => {}
        }
    }
    removed
}

/// One action with its `/Next` chain cleaned: `None` when the action itself jumps to
/// a deleted page.
fn prune_action_value(doc: &Document, value: &Object, deleted: &HashSet<ObjectId>) -> Option<Object> {
    let (id, dict) = match value {
        Object::Reference(id) => match doc.get_dictionary(*id) {
            Ok(dict) => (Some(*id), dict.clone()),
            Err(_) => return Some(value.clone()),
        },
        Object::Dictionary(dict) => (None, dict.clone()),
        other => return Some(other.clone()),
    };
    if let Ok(dest) = dict.get(b"D") {
        if targets_deleted_page(doc, dest, deleted) {
            return None;
        }
    }
    let mut pruned = dict.clone();
    match dict.get(b"Next").ok().cloned() {
        Some(Object::Array(steps)) => {
            let kept: Vec<Object> = steps.iter().filter_map(|step| prune_action_value(doc, step, deleted)).collect();
            if kept.is_empty() {
                pruned.remove(b"Next");
            } else {
                pruned.set("Next", Object::Array(kept));
            }
        }
        Some(next) => match prune_action_value(doc, &next, deleted) {
            Some(step) => pruned.set("Next", step),
            None => {
                pruned.remove(b"Next");
            }
        },
        None => {}
    }
    match id {
        Some(_) => {
            // The action is an indirect object; the caller writes it back.
            Some(Object::Dictionary(pruned))
        }
        None => Some(Object::Dictionary(pruned)),
    }
}

/// True when this action, or any step of its `/Next` chain, jumps to a deleted page.
fn action_targets_deleted(doc: &Document, value: &Object, deleted: &HashSet<ObjectId>) -> bool {
    let dict = match value {
        Object::Reference(id) => match doc.get_dictionary(*id) {
            Ok(dict) => dict.clone(),
            Err(_) => return false,
        },
        Object::Dictionary(dict) => dict.clone(),
        _ => return false,
    };
    if let Ok(dest) = dict.get(b"D") {
        if targets_deleted_page(doc, dest, deleted) {
            return true;
        }
    }
    match dict.get(b"Next").ok() {
        Some(Object::Array(steps)) => steps.iter().any(|step| action_targets_deleted(doc, step, deleted)),
        Some(next) => action_targets_deleted(doc, next, deleted),
        None => false,
    }
}

/// Clears `/SE` on outline items whose structure element no longer describes a page,
/// and prunes an `/A` that names a deleted page.
fn prune_outline_references(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"Outlines").ok().cloned())
        .and_then(|value| value.as_reference().ok())
    else {
        return 0;
    };
    let mut removed = 0;
    let mut current = doc.get_dictionary(root).ok().and_then(|dict| dict.get(b"First").ok().cloned());
    let mut hops = 0;
    while let Some(value) = current {
        hops += 1;
        if hops > 10_000 {
            break;
        }
        let Some(id) = value.as_reference().ok() else { break };
        let Ok(entry) = doc.get_dictionary(id).cloned() else { break };
        let next = entry.get(b"Next").ok().cloned();
        let mut clear_se = false;
        let mut clear_actions = false;
        if let Ok(se) = entry.get(b"SE").and_then(Object::as_reference) {
            clear_se = match doc.get_dictionary(se) {
                Ok(element) => match element.get(b"Pg").and_then(Object::as_reference) {
                    Ok(page) => {
                        deleted.contains(&page) || !doc.get_pages().values().any(|candidate| *candidate == page)
                    }
                    Err(_) => false,
                },
                Err(_) => true,
            };
        }
        if let Ok(action) = entry.get(b"A") {
            clear_actions = action_targets_deleted(doc, action, deleted);
        }
        if clear_se || clear_actions {
            if let Ok(dict) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                if clear_se {
                    dict.remove(b"SE");
                    removed += 1;
                }
                if clear_actions {
                    dict.remove(b"A");
                    removed += 1;
                }
            }
        }
        current = next;
    }
    removed
}

/// Unused import guard: `destination_page` keeps the crate-level helper honest.
#[allow(dead_code)]
fn _keeps_destination_page_used(doc: &Document, value: &Object) -> Option<u32> {
    destination_page(doc, value)
}

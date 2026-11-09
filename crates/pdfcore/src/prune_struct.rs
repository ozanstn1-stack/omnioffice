//! Taking a deleted page out of the structure tree.
//!
//! A tagged document describes every page through structure elements, each bound to
//! its page with `/Pg` (ISO 32000-1 14.7.2), and indexes the marked content through
//! the page's `/StructParents` key in the tree's `/ParentTree` (14.7.4.4). Leaving
//! those behind keeps the page object - and the text on it - reachable in a file the
//! user believes they cut down.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

/// Removes the structure elements that describe the deleted pages, and their keys
/// from the parent tree. Returns how many elements were dropped.
pub fn prune_structure(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    if deleted.is_empty() {
        return 0;
    }
    let Some(root) = doc.catalog().ok().and_then(|catalog| catalog.get(b"StructTreeRoot").ok().cloned()) else {
        return 0;
    };
    let root_id = match &root {
        Object::Reference(id) => Some(*id),
        _ => None,
    };
    let removed = match root_id {
        Some(id) => prune_kids_of(doc, id, deleted),
        None => 0,
    };
    prune_parent_tree(doc, root_id, deleted);
    removed
}

/// Walks one structure element's `/K` and drops the children that describe a
/// deleted page, then the element itself when nothing is left.
fn prune_kids_of(doc: &mut Document, id: ObjectId, deleted: &HashSet<ObjectId>) -> u32 {
    let Ok(dict) = doc.get_dictionary(id).cloned() else { return 0 };
    let mut removed = 0;
    if let Ok(kids) = dict.get(b"K").and_then(Object::as_array) {
        let kids = kids.clone();
        let mut kept: Vec<Object> = Vec::with_capacity(kids.len());
        for kid in kids {
            match kid.as_reference().ok() {
                Some(child) => {
                    if let Ok(child_dict) = doc.get_dictionary(child).cloned() {
                        if describes_deleted(&child_dict, deleted) {
                            removed += 1;
                            continue;
                        }
                        removed += prune_kids_of(doc, child, deleted);
                        kept.push(kid);
                    } else {
                        kept.push(kid);
                    }
                }
                // Marked-content and object-reference dictionaries point at a page
                // directly; a child that carries no `/Pg` of its own inherits it.
                None => {
                    if describes_deleted_object(&kid, deleted) {
                        removed += 1;
                    } else {
                        kept.push(kid);
                    }
                }
            }
        }
        if kept.len() != dict.get(b"K").and_then(Object::as_array).map(|items| items.len()).unwrap_or(0) {
            if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                if kept.is_empty() {
                    entry.set("K", Object::Integer(0));
                } else {
                    entry.set("K", Object::Array(kept));
                }
            }
        }
    }
    removed
}

fn describes_deleted(dict: &Dictionary, deleted: &HashSet<ObjectId>) -> bool {
    match dict.get(b"Pg").and_then(Object::as_reference) {
        Ok(page) => deleted.contains(&page),
        Err(_) => false,
    }
}

fn describes_deleted_object(object: &Object, deleted: &HashSet<ObjectId>) -> bool {
    match object {
        Object::Dictionary(dict) => describes_deleted(dict, deleted),
        _ => false,
    }
}

/// Drops the parent-tree entries whose page is one of the deleted ones. A key was
/// the page's `/StructParents`, and the value is what that page's content pointed
/// at; both are meaningless once the page is gone.
fn prune_parent_tree(doc: &mut Document, root: Option<ObjectId>, deleted: &HashSet<ObjectId>) {
    let Some(root) = root else { return };
    let tree = doc.get_dictionary(root).ok().and_then(|dict| dict.get(b"ParentTree").ok().cloned());
    let Some(tree) = tree else { return };
    // Which keys belonged to the deleted pages was recorded on the pages themselves,
    // and they are no longer in the tree, so every key whose value mentions a
    // deleted page is dropped.
    let mut seen = HashSet::new();
    prune_parent_node(doc, &tree, deleted, &mut seen);
}

fn prune_parent_node(doc: &mut Document, node: &Object, deleted: &HashSet<ObjectId>, seen: &mut HashSet<ObjectId>) {
    let id = match node {
        Object::Reference(id) => {
            if !seen.insert(*id) {
                return;
            }
            Some(*id)
        }
        Object::Dictionary(_) => None,
        _ => return,
    };
    let dict = match id {
        Some(id) => doc.get_dictionary(id).ok().cloned(),
        None => match node {
            Object::Dictionary(dict) => Some(dict.clone()),
            _ => None,
        },
    };
    let Some(dict) = dict else { return };
    if let Ok(nums) = dict.get(b"Nums").and_then(Object::as_array) {
        let nums = nums.clone();
        let mut kept: Vec<Object> = Vec::with_capacity(nums.len());
        for pair in nums.chunks(2) {
            let [key, value] = pair else { continue };
            if mentions_deleted(value, deleted) {
                continue;
            }
            kept.push(key.clone());
            kept.push(value.clone());
        }
        if kept.len() != nums.len() {
            if let Some(id) = id {
                if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                    entry.set("Nums", Object::Array(kept));
                }
            }
        }
    }
    for kid in dict.get(b"Kids").and_then(Object::as_array).cloned().unwrap_or_default() {
        prune_parent_node(doc, &kid, deleted, seen);
    }
}

fn mentions_deleted(object: &Object, deleted: &HashSet<ObjectId>) -> bool {
    match object {
        Object::Reference(id) => deleted.contains(id),
        Object::Array(items) => items.iter().any(|item| mentions_deleted(item, deleted)),
        Object::Dictionary(dict) => {
            dict.iter().any(|(_, value)| mentions_deleted(value, deleted))
                || dict.get(b"Pg").and_then(Object::as_reference).map(|id| deleted.contains(&id)).unwrap_or(false)
        }
        _ => false,
    }
}

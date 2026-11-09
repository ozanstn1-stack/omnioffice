//! Form fields and article threads that pointed at a deleted page.
//!
//! A widget annotation names its page with `/P` (ISO 32000-1 12.5.6.19) and the form
//! lists it through `/Fields`/`/Kids` (12.7.3.1); an article thread lists its beads
//! and each bead names its page (12.4.3). A field value or a bead left behind keeps
//! the deleted page object reachable, and with it the content on that page.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

fn on_deleted_page(dict: &Dictionary, deleted: &HashSet<ObjectId>) -> bool {
    match dict.get(b"P").and_then(Object::as_reference) {
        Ok(page) => deleted.contains(&page),
        Err(_) => false,
    }
}

fn is_widget(dict: &Dictionary) -> bool {
    dict.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"") == b"Widget"
}

/// Drops the form fields - and the widgets inside them - that sat on a deleted page.
pub fn prune_forms(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    if deleted.is_empty() {
        return 0;
    }
    let Some(form) = doc.catalog().ok().and_then(|catalog| catalog.get(b"AcroForm").ok().cloned()) else {
        return 0;
    };
    let (form_id, fields) = match &form {
        Object::Reference(id) => match doc.get_dictionary(*id).ok().and_then(|dict| dict.get(b"Fields").ok()) {
            Some(value) => (Some(*id), value.as_array().ok().cloned().unwrap_or_default()),
            None => return 0,
        },
        Object::Dictionary(dict) => match dict.get(b"Fields").ok().and_then(|value| value.as_array().ok()) {
            Some(items) => (None, items.clone()),
            None => return 0,
        },
        _ => return 0,
    };

    let mut removed = 0;
    let mut kept: Vec<Object> = Vec::with_capacity(fields.len());
    for field in fields {
        let Some(id) = field.as_reference().ok() else {
            kept.push(field);
            continue;
        };
        let Ok(dict) = doc.get_dictionary(id).cloned() else {
            kept.push(field);
            continue;
        };
        // A leaf field that is itself the widget of the deleted page.
        if is_widget(&dict) && on_deleted_page(&dict, deleted) {
            removed += 1;
            continue;
        }
        if let Ok(kids) = dict.get(b"Kids").and_then(Object::as_array) {
            let kids = kids.clone();
            let mut surviving: Vec<Object> = Vec::with_capacity(kids.len());
            for kid in &kids {
                let dead = kid
                    .as_reference()
                    .ok()
                    .and_then(|kid_id| doc.get_dictionary(kid_id).ok())
                    .map(|kid_dict| is_widget(kid_dict) && on_deleted_page(kid_dict, deleted))
                    .unwrap_or(false);
                if dead {
                    removed += 1;
                } else {
                    surviving.push(kid.clone());
                }
            }
            if surviving.len() != kids.len() {
                if surviving.is_empty() {
                    // Every widget of this field was on the deleted page: the field
                    // itself goes too, or its value would keep the page reachable.
                    removed += 1;
                    continue;
                }
                if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                    entry.set("Kids", Object::Array(surviving));
                }
            }
        }
        kept.push(field);
    }

    if removed > 0 {
        if let Some(id) = form_id {
            if let Ok(entry) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                entry.set("Fields", Object::Array(kept));
            }
        }
    }
    removed
}

/// Rewrites the article threads so no bead points at a deleted page.
pub fn prune_threads(doc: &mut Document, deleted: &HashSet<ObjectId>) -> u32 {
    if deleted.is_empty() {
        return 0;
    }
    let root = match doc.trailer.get(b"Root").and_then(Object::as_reference) {
        Ok(root) => root,
        Err(_) => return 0,
    };
    let Some(items) = doc
        .get_dictionary(root)
        .ok()
        .and_then(|catalog| catalog.get(b"Threads").ok())
        .and_then(|value| value.as_array().ok())
        .cloned()
    else {
        return 0;
    };

    let mut removed = 0;
    let mut kept: Vec<Object> = Vec::with_capacity(items.len());
    for item in &items {
        let Some(thread_id) = item.as_reference().ok() else {
            kept.push(item.clone());
            continue;
        };
        let Some(first) = doc
            .get_dictionary(thread_id)
            .ok()
            .and_then(|thread| thread.get(b"F").ok())
            .and_then(|value| value.as_reference().ok())
        else {
            kept.push(item.clone());
            continue;
        };
        // Walk the bead ring once; it is a ring, so stop when it closes.
        let mut order: Vec<ObjectId> = Vec::new();
        let mut current = Some(first);
        while let Some(id) = current {
            if order.contains(&id) || order.len() > 10_000 {
                break;
            }
            order.push(id);
            current = doc
                .get_dictionary(id)
                .ok()
                .and_then(|bead| bead.get(b"N").ok())
                .and_then(|value| value.as_reference().ok());
        }
        let surviving: Vec<ObjectId> = order
            .iter()
            .copied()
            .filter(|id| {
                doc.get_dictionary(*id)
                    .ok()
                    .and_then(|bead| bead.get(b"P").ok())
                    .and_then(|value| value.as_reference().ok())
                    .map(|page| !deleted.contains(&page))
                    .unwrap_or(false)
            })
            .collect();
        if surviving.len() == order.len() {
            kept.push(item.clone());
            continue;
        }
        removed += (order.len() - surviving.len()) as u32;
        if surviving.is_empty() {
            continue;
        }
        for (index, id) in surviving.iter().enumerate() {
            let next = surviving[(index + 1) % surviving.len()];
            let previous = surviving[(index + surviving.len() - 1) % surviving.len()];
            if let Ok(bead) = doc.get_object_mut(*id).and_then(Object::as_dict_mut) {
                bead.set("N", Object::Reference(next));
                bead.set("V", Object::Reference(previous));
            }
        }
        if let Ok(thread) = doc.get_object_mut(thread_id).and_then(Object::as_dict_mut) {
            thread.set("F", Object::Reference(surviving[0]));
            thread.set("I", Object::Reference(surviving[0]));
        }
        kept.push(item.clone());
    }

    if removed > 0 {
        if let Ok(catalog) = doc.get_object_mut(root).and_then(Object::as_dict_mut) {
            if kept.is_empty() {
                catalog.remove(b"Threads");
            } else {
                catalog.set("Threads", Object::Array(kept));
            }
        }
    }
    removed
}

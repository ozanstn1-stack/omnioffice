//! Outlines (bookmarks) in a source document, and readers for the imposed one.
//!
//! Kept apart from `link_fixtures.rs` so that module keeps its line budget; the
//! builder works on a written file so it needs nothing from the page builder.

#![allow(dead_code)]

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use std::path::Path;

/// One outline entry, optionally with children.
pub struct OutlineSpec {
    pub title: &'static str,
    /// A destination array or an action dictionary; `None` makes a bare heading.
    pub target: Option<Object>,
    pub open: bool,
    pub children: Vec<OutlineSpec>,
    pub color: Option<[f64; 3]>,
    pub flags: Option<i64>,
}

impl OutlineSpec {
    pub fn new(title: &'static str, target: Option<Object>) -> Self {
        OutlineSpec { title, target, open: false, children: Vec::new(), color: None, flags: None }
    }

    pub fn open(mut self) -> Self {
        self.open = true;
        self
    }

    pub fn child(mut self, child: OutlineSpec) -> Self {
        self.children.push(child);
        self
    }

    pub fn colored(mut self, color: [f64; 3]) -> Self {
        self.color = Some(color);
        self
    }

    pub fn italic(mut self) -> Self {
        self.flags = Some(1);
        self
    }
}

/// Adds an outline tree to the document stored at `path`; `page_mode` sets the
/// catalog entry a viewer uses to open the outline panel.
pub fn add_outline(path: &Path, specs: &[OutlineSpec], page_mode: Option<&str>) {
    let mut doc = Document::load(path).expect("load fixture");
    let catalog_id = catalog_id(&doc);
    let root = doc.new_object_id();
    if let Some((first, last, visible)) = write_level(&mut doc, root, specs) {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Outlines".to_vec()));
        dict.set("First", Object::Reference(first));
        dict.set("Last", Object::Reference(last));
        dict.set("Count", Object::Integer(visible));
        doc.objects.insert(root, Object::Dictionary(dict));
        doc.get_dictionary_mut(catalog_id).expect("catalog").set("Outlines", Object::Reference(root));
        if let Some(mode) = page_mode {
            doc.get_dictionary_mut(catalog_id)
                .expect("catalog")
                .set("PageMode", Object::Name(mode.as_bytes().to_vec()));
        }
    }
    doc.save(path).expect("save fixture");
}

/// The catalog object id of a written document.
pub fn catalog_id(doc: &Document) -> ObjectId {
    doc.trailer.get(b"Root").expect("root").as_reference().expect("root ref")
}

/// Object id of the output `/Outlines` root, when it has one.
pub fn outline_root_id(doc: &Document) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    catalog.get(b"Outlines").ok()?.as_reference().ok()
}

/// The `/Outlines` root dictionary of an output document, when it has one.
pub fn outline_root(doc: &Document) -> Option<Dictionary> {
    let catalog = doc.catalog().ok()?;
    let outlines = match catalog.get(b"Outlines").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?.clone(),
        Object::Dictionary(dict) => dict.clone(),
        _ => return None,
    };
    Some(outlines)
}

/// Object ids along a `/Next` chain starting at `first`.
pub fn chain_ids(doc: &Document, first: Option<ObjectId>) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    let mut current = first;
    while let Some(id) = current {
        if ids.contains(&id) {
            break;
        }
        ids.push(id);
        current = match doc.get_dictionary(id).ok().and_then(|dict| dict.get(b"Next").ok().cloned()) {
            Some(Object::Reference(next)) => Some(next),
            _ => None,
        };
    }
    ids
}

/// The outline entries at the top level, in order.
pub fn top_items(doc: &Document) -> Vec<Dictionary> {
    let Some(root) = outline_root(doc) else { return Vec::new() };
    let first = root.get(b"First").ok().and_then(|value| value.as_reference().ok());
    chain_ids(doc, first).into_iter().filter_map(|id| doc.get_dictionary(id).ok().cloned()).collect()
}

/// The children of one entry, in order.
pub fn children_of(doc: &Document, item: &Dictionary) -> Vec<Dictionary> {
    let first = item.get(b"First").ok().and_then(|value| value.as_reference().ok());
    chain_ids(doc, first).into_iter().filter_map(|id| doc.get_dictionary(id).ok().cloned()).collect()
}

/// Every entry below one item, depth first.
pub fn descendants(doc: &Document, item: &Dictionary) -> Vec<Dictionary> {
    let mut out = Vec::new();
    for child in children_of(doc, item) {
        out.push(child.clone());
        out.extend(descendants(doc, &child));
    }
    out
}

/// A `/Dest` array as `[page reference, view name, numbers...]`.
pub fn dest_of(doc: &Document, item: &Dictionary) -> Option<(ObjectId, Vec<u8>, Vec<f64>)> {
    let target = match item.get(b"Dest").ok()? {
        Object::Array(items) => items.clone(),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?.clone(),
        _ => return None,
    };
    let page = target.first()?.as_reference().ok()?;
    let view = target.get(1)?.as_name().ok()?.to_vec();
    let numbers = target[2..].iter().filter_map(number).collect();
    Some((page, view, numbers))
}

/// The value of an integer key, when present.
pub fn integer(dict: &Dictionary, key: &str) -> Option<i64> {
    dict.get(key.as_bytes()).ok().and_then(|value| value.as_i64().ok())
}

pub fn number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(*value as f64),
        _ => None,
    }
}

/// A destination array `[page /XYZ left top zoom]`.
pub fn xyz(page: ObjectId, left: f64, top: f64, zoom: Option<i64>) -> Object {
    let zoom = zoom.map(Object::Integer).unwrap_or(Object::Null);
    Object::Array(vec![
        Object::Reference(page),
        Object::Name(b"XYZ".to_vec()),
        Object::Real(left as f32),
        Object::Real(top as f32),
        zoom,
    ])
}

/// A destination array `[page /Fit]` (and the other view shapes).
pub fn view(page: ObjectId, kind: &str) -> Object {
    Object::Array(vec![Object::Reference(page), Object::Name(kind.as_bytes().to_vec())])
}

/// `[page /FitR left bottom right top]`.
pub fn fit_r(page: ObjectId, rect: [f64; 4]) -> Object {
    let mut items = vec![Object::Reference(page), Object::Name(b"FitR".to_vec())];
    items.extend(rect.iter().map(|value| Object::Real(*value as f32)));
    Object::Array(items)
}

pub fn goto_action(dest: Object) -> Object {
    Object::Dictionary(dictionary! { "S" => "GoTo", "D" => dest })
}

pub fn uri_action(url: &str) -> Object {
    Object::Dictionary(dictionary! { "S" => "URI", "URI" => Object::string_literal(url) })
}

fn write_level(doc: &mut Document, parent: ObjectId, specs: &[OutlineSpec]) -> Option<(ObjectId, ObjectId, i64)> {
    let ids: Vec<ObjectId> = specs.iter().map(|_| doc.new_object_id()).collect();
    let mut visible = 0i64;
    for (index, spec) in specs.iter().enumerate() {
        let id = ids[index];
        let mut dict = Dictionary::new();
        dict.set("Title", Object::string_literal(spec.title));
        dict.set("Parent", Object::Reference(parent));
        if index > 0 {
            dict.set("Prev", Object::Reference(ids[index - 1]));
        }
        if index + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[index + 1]));
        }
        if let Some(target) = &spec.target {
            if target.as_dict().is_ok() {
                dict.set("A", target.clone());
            } else {
                dict.set("Dest", target.clone());
            }
        }
        if let Some(color) = spec.color {
            dict.set("C", Object::Array(color.iter().map(|value| Object::Real(*value as f32)).collect()));
        }
        if let Some(flags) = spec.flags {
            dict.set("F", Object::Integer(flags));
        }
        if !spec.children.is_empty() {
            if let Some((first, last, child_visible)) = write_level(doc, id, &spec.children) {
                dict.set("First", Object::Reference(first));
                dict.set("Last", Object::Reference(last));
                let child_total = spec.children.len() as i64;
                dict.set(
                    "Count",
                    Object::Integer(if spec.open { child_visible } else { -child_visible.max(child_total) }),
                );
                if spec.open {
                    visible += child_visible;
                }
            }
        }
        doc.objects.insert(id, Object::Dictionary(dict));
        visible += 1;
    }
    Some((*ids.first()?, *ids.last()?, visible))
}

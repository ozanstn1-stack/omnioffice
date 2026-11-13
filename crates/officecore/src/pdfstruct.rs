//! Tagged-PDF structure tree builder (PDF/UA-oriented hierarchy).
//!
//! Serializes nested [`StructContainer`]s (`Table`, `TR`, `TH`, `TD`, ...) and
//! leaf [`StructItem`]s (`P`, `H1`..`H6`, `Figure`, ...) under a `Document`
//! root, linking every leaf back to its page marked-content identifier via
//! `ParentTree`.

use crate::pdfcanvas::{BuiltPage, StructChild};
use lopdf::{dictionary, Document, Object, ObjectId};

/// Encodes a PDF text string, using UTF-16BE with BOM for non-ASCII input.
pub(crate) fn pdf_text_object(value: &str) -> Object {
    if value.is_ascii() {
        Object::string_literal(value.to_string())
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in value.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::string_literal(bytes)
    }
}

/// Inserts `StructTreeRoot`, `Document`, hierarchical containers, leaf
/// structure elements and `ParentTree` into `doc`, returning the root ID.
pub(crate) fn write_structure_tree(doc: &mut Document, pages: &[BuiltPage], page_ids: &[ObjectId]) -> ObjectId {
    let struct_root_id = doc.new_object_id();
    let document_elem_id = doc.new_object_id();
    let mut document_kids: Vec<Object> = Vec::new();
    let mut parent_tree_nums: Vec<Object> = Vec::new();

    for (page_index, (page, &page_id)) in pages.iter().zip(page_ids.iter()).enumerate() {
        if page.struct_items.is_empty() && page.struct_containers.is_empty() {
            continue;
        }
        let item_ids: Vec<ObjectId> = (0..page.struct_items.len()).map(|_| doc.new_object_id()).collect();
        let container_ids: Vec<ObjectId> = (0..page.struct_containers.len()).map(|_| doc.new_object_id()).collect();
        let mut item_parents = vec![document_elem_id; page.struct_items.len()];
        let mut container_parents = vec![document_elem_id; page.struct_containers.len()];

        for (container_index, container) in page.struct_containers.iter().enumerate() {
            let parent_id = container_ids[container_index];
            for child in &container.children {
                match child {
                    StructChild::Container(child_index) => {
                        if let Some(slot) = container_parents.get_mut(*child_index) {
                            *slot = parent_id;
                        }
                    }
                    StructChild::Item(mcid) => {
                        if let Some(slot) = item_parents.get_mut(*mcid as usize) {
                            *slot = parent_id;
                        }
                    }
                }
            }
        }

        for (container_index, container) in page.struct_containers.iter().enumerate() {
            let kids: Vec<Object> = container
                .children
                .iter()
                .filter_map(|child| match child {
                    StructChild::Container(child_index) => {
                        container_ids.get(*child_index).copied().map(Object::Reference)
                    }
                    StructChild::Item(mcid) => item_ids.get(*mcid as usize).copied().map(Object::Reference),
                })
                .collect();
            let elem = dictionary! {
                "Type" => "StructElem",
                "S" => Object::Name(container.tag.clone().into_bytes()),
                "P" => Object::Reference(container_parents[container_index]),
                "Pg" => Object::Reference(page_id),
                "K" => Object::Array(kids),
            };
            doc.objects.insert(container_ids[container_index], Object::Dictionary(elem));
        }

        for (mcid, item) in page.struct_items.iter().enumerate() {
            let mut elem = dictionary! {
                "Type" => "StructElem",
                "S" => Object::Name(item.tag.clone().into_bytes()),
                "P" => Object::Reference(item_parents[mcid]),
                "Pg" => Object::Reference(page_id),
                "K" => Object::Dictionary(dictionary! {
                    "Type" => "MCR",
                    "Pg" => Object::Reference(page_id),
                    "MCID" => mcid as i64,
                }),
            };
            if let Some(alt) = &item.alt {
                if !alt.trim().is_empty() {
                    elem.set("Alt", pdf_text_object(alt));
                }
            }
            doc.objects.insert(item_ids[mcid], Object::Dictionary(elem));
        }

        if page.struct_roots.is_empty() {
            for &id in &item_ids {
                document_kids.push(Object::Reference(id));
            }
        } else {
            for root in &page.struct_roots {
                match root {
                    StructChild::Container(child_index) => {
                        if let Some(&id) = container_ids.get(*child_index) {
                            document_kids.push(Object::Reference(id));
                        }
                    }
                    StructChild::Item(mcid) => {
                        if let Some(&id) = item_ids.get(*mcid as usize) {
                            document_kids.push(Object::Reference(id));
                        }
                    }
                }
            }
        }

        if !item_ids.is_empty() {
            let page_elems: Vec<Object> = item_ids.iter().copied().map(Object::Reference).collect();
            parent_tree_nums.push((page_index as i64).into());
            parent_tree_nums.push(Object::Array(page_elems));
        }
    }

    let parent_tree_id = doc.add_object(dictionary! { "Nums" => Object::Array(parent_tree_nums) });
    doc.objects.insert(
        document_elem_id,
        Object::Dictionary(dictionary! {
            "Type" => "StructElem",
            "S" => "Document",
            "P" => Object::Reference(struct_root_id),
            "K" => Object::Array(document_kids),
        }),
    );
    doc.objects.insert(
        struct_root_id,
        Object::Dictionary(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => vec![Object::Reference(document_elem_id)],
            "ParentTree" => Object::Reference(parent_tree_id),
            "ParentTreeNextKey" => pages.len() as i64,
        }),
    );
    struct_root_id
}

//! Structural checks for a rebuilt outline, shared by the N-up outline tests.

#![allow(dead_code)]

use crate::outlines::{integer, outline_root_id};
use lopdf::{Document, ObjectId};

/// Walks the outline and checks every link and the `/Count` arithmetic.
pub fn check_outline(doc: &Document) -> (i64, i64) {
    let Some(root_id) = outline_root_id(doc) else { return (0, 0) };
    let root = doc.get_dictionary(root_id).expect("outline root");
    let first = root.get(b"First").ok().and_then(|value| value.as_reference().ok());
    let (ids, total) = check_chain(doc, first, root_id);
    assert_eq!(root.get(b"Count").unwrap().as_i64().unwrap(), visible(doc, &ids), "root count");
    if ids.is_empty() {
        assert!(root.get(b"Last").is_err(), "an empty outline has no /Last");
    } else {
        assert_eq!(root.get(b"Last").unwrap().as_reference().unwrap(), *ids.last().unwrap(), "root last");
    }
    (ids.len() as i64, total)
}

fn check_chain(doc: &Document, first: Option<ObjectId>, parent: ObjectId) -> (Vec<ObjectId>, i64) {
    let mut ids: Vec<ObjectId> = Vec::new();
    let mut total = 0i64;
    let mut current = first;
    while let Some(id) = current {
        assert!(!ids.contains(&id), "the chain loops at {id:?}");
        let dict = doc.get_dictionary(id).expect("outline item");
        assert_eq!(dict.get(b"Parent").unwrap().as_reference().unwrap(), parent, "parent link");
        if let Some(previous) = ids.last() {
            let previous_id = *previous;
            let previous_dict = doc.get_dictionary(previous_id).expect("previous item");
            assert_eq!(previous_dict.get(b"Next").unwrap().as_reference().unwrap(), id, "next link");
            assert_eq!(dict.get(b"Prev").unwrap().as_reference().unwrap(), previous_id, "prev link");
        } else {
            assert!(dict.get(b"Prev").is_err(), "the first item has no /Prev");
        }
        let child_first = dict.get(b"First").ok().and_then(|value| value.as_reference().ok());
        let child_total = match child_first {
            Some(child_first) => {
                let (children, child_total) = check_chain(doc, Some(child_first), id);
                assert!(!children.is_empty());
                assert_eq!(dict.get(b"Last").unwrap().as_reference().unwrap(), *children.last().unwrap(), "last child");
                let count = dict.get(b"Count").unwrap().as_i64().unwrap();
                let expected = if count > 0 { visible(doc, &children) } else { child_total };
                assert_eq!(count.abs(), expected, "count of {id:?}");
                child_total
            }
            None => {
                assert!(dict.get(b"Count").is_err(), "a leaf has no /Count");
                assert!(dict.get(b"Last").is_err());
                0
            }
        };
        ids.push(id);
        total += 1 + child_total;
        current = dict.get(b"Next").ok().and_then(|value| value.as_reference().ok());
    }
    (ids, total)
}

/// How many entries a reader sees at one level: every sibling plus the visible
/// descendants of the open ones.
fn visible(doc: &Document, ids: &[ObjectId]) -> i64 {
    let mut count = 0;
    for id in ids {
        count += 1;
        let dict = doc.get_dictionary(*id).expect("item");
        if integer(dict, "Count").is_some_and(|count| count > 0) {
            let first = dict.get(b"First").ok().and_then(|value| value.as_reference().ok());
            let children: Vec<ObjectId> = match first {
                Some(first) => {
                    let mut found = Vec::new();
                    let mut current = Some(first);
                    while let Some(id) = current {
                        found.push(id);
                        current = doc
                            .get_dictionary(id)
                            .ok()
                            .and_then(|dict| dict.get(b"Next").ok())
                            .and_then(|value| value.as_reference().ok());
                    }
                    found
                }
                None => Vec::new(),
            };
            count += visible(doc, &children);
        }
    }
    count
}

//! Deleting annotations (plan 4.6b): indexes refer to the list before the
//! batch, and a deleted comment takes its popup and replies with it.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdfcore::annotate::{edit_annotations, list_annotations, AnnotationAction, AnnotationEditItem};

fn reals(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn square(x: f64) -> Dictionary {
    dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "F" => 4,
        "Rect" => reals(&[x, 100.0, x + 50.0, 150.0]),
        "C" => reals(&[0.0, 0.0, 1.0]),
    }
}

fn note(contents: &str) -> Dictionary {
    dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "F" => 4,
        "Rect" => reals(&[300.0, 600.0, 320.0, 620.0]),
        "Contents" => Object::string_literal(contents),
    }
}

/// A page with: [0] a comment, [1] its popup, [2] a reply, [3] a reply to the
/// reply, [4] and [5] two squares. Returns the bytes and the object ids.
fn thread_doc() -> (Vec<u8>, Vec<ObjectId>) {
    let mut doc = build_text_doc(1, "Thread", "Comment thread");
    let page_id = doc.get_pages().get(&1).copied().expect("page");
    let comment = doc.add_object(note("Please check this figure"));
    let mut popup = dictionary! {
        "Type" => "Annot", "Subtype" => "Popup",
        "Rect" => reals(&[320.0, 500.0, 480.0, 600.0]),
    };
    popup.set("Parent", Object::Reference(comment));
    let popup = doc.add_object(popup);
    doc.get_dictionary_mut(comment).unwrap().set("Popup", Object::Reference(popup));
    let mut reply = note("Checked, it is right");
    reply.set("IRT", Object::Reference(comment));
    let reply = doc.add_object(reply);
    let mut second = note("Thanks");
    second.set("IRT", Object::Reference(reply));
    let second = doc.add_object(second);
    let left = doc.add_object(square(100.0));
    let right = doc.add_object(square(400.0));
    let ids = vec![comment, popup, reply, second, left, right];
    let refs = ids.iter().map(|id| Object::Reference(*id)).collect();
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Annots", Object::Array(refs));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    (bytes, ids)
}

fn page_annots(bytes: &[u8]) -> Vec<ObjectId> {
    let doc = Document::load_mem(bytes).expect("reload");
    let page_id = doc.get_pages().get(&1).copied().unwrap();
    match doc.get_dictionary(page_id).unwrap().get(b"Annots").unwrap() {
        Object::Array(items) => items.iter().filter_map(|item| item.as_reference().ok()).collect(),
        _ => Vec::new(),
    }
}

fn left_edge(bytes: &[u8], id: ObjectId) -> f32 {
    let doc = Document::load_mem(bytes).expect("reload");
    let rect = doc.get_dictionary(id).unwrap().get(b"Rect").unwrap().as_array().unwrap().clone();
    rect[0].as_float().unwrap()
}

#[test]
fn a_delete_does_not_shift_the_index_of_a_later_edit() {
    let (bytes, ids) = thread_doc();
    // The app sends edits in the order they were made: delete the first
    // square, then move the second one by the index it was listed under.
    let items = vec![
        AnnotationEditItem { page: 1, index: 4, action: AnnotationAction::Delete },
        AnnotationEditItem { page: 1, index: 5, action: AnnotationAction::Move { dx: 30.0, dy: 0.0 } },
    ];
    let (edited, report) = edit_annotations(&bytes, &items).expect("edit");
    assert_eq!((report.edited, report.deleted), (1, 1), "{:?}", report.warnings);
    assert!(edited.starts_with(&bytes), "still an appended revision");
    assert!((left_edge(&edited, ids[5]) - 430.0).abs() < 0.01, "the listed square moved");
    let mut kept = ids[..4].to_vec();
    kept.push(ids[5]);
    assert_eq!(page_annots(&edited), kept, "only the first square is gone");
}

#[test]
fn deleting_a_comment_removes_its_popup_and_every_reply() {
    let (bytes, ids) = thread_doc();
    let items = vec![AnnotationEditItem { page: 1, index: 0, action: AnnotationAction::Delete }];
    let (edited, report) = edit_annotations(&bytes, &items).expect("edit");
    assert_eq!(report.deleted, 1);
    assert_eq!(page_annots(&edited), vec![ids[4], ids[5]], "popup, reply and reply-to-reply go with it");
    let listed = list_annotations(&Document::load_mem(&edited).unwrap());
    assert_eq!(listed.len(), 2);

    // Deleting only a reply keeps the comment and its popup.
    let items = vec![AnnotationEditItem { page: 1, index: 2, action: AnnotationAction::Delete }];
    let (edited, _) = edit_annotations(&bytes, &items).expect("edit");
    assert_eq!(page_annots(&edited), vec![ids[0], ids[1], ids[4], ids[5]]);
}

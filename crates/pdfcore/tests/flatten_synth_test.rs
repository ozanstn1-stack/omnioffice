//! Flattening markup that has no appearance stream (plan 4.6a): it is drawn
//! from its geometry, the way imposition already draws it, and burned in.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdfcore::flatten::{flatten_pdf, FlattenOptions, FlattenReport};
use pdfcore::progress::CancelToken;

fn reals(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document carrying the given annotations, none with an `/AP`.
fn doc_with(annots: Vec<Dictionary>) -> Document {
    let mut doc = build_text_doc(1, "Notes", "Markup document");
    let page_id = doc.get_pages().get(&1).copied().expect("page");
    let refs: Vec<Object> = annots.into_iter().map(|annot| Object::Reference(doc.add_object(annot))).collect();
    doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Annots", Object::Array(refs));
    doc
}

fn highlight() -> Dictionary {
    dictionary! {
        "Type" => "Annot", "Subtype" => "Highlight", "F" => 4,
        "Rect" => reals(&[100.0, 700.0, 300.0, 720.0]),
        "QuadPoints" => reals(&[100.0, 720.0, 300.0, 720.0, 100.0, 700.0, 300.0, 700.0]),
        "C" => reals(&[1.0, 1.0, 0.0]),
    }
}

fn square() -> Dictionary {
    dictionary! {
        "Type" => "Annot", "Subtype" => "Square", "F" => 4,
        "Rect" => reals(&[150.0, 400.0, 350.0, 500.0]),
        "C" => reals(&[0.0, 0.0, 1.0]),
    }
}

fn flatten(doc: &mut Document) -> (FlattenReport, Document) {
    let dir = TestDir::new();
    let source = dir.path("markup.pdf");
    write_doc(doc, &source);
    let output = dir.path("flat.pdf");
    let report =
        flatten_pdf(&source, &output, &FlattenOptions::default(), &no_progress, &CancelToken::new()).expect("flatten");
    (report, Document::load(&output).expect("reopen"))
}

fn page_annots(doc: &Document) -> usize {
    let page_id = doc.get_pages().get(&1).copied().unwrap();
    match doc.get_dictionary(page_id).unwrap().get(b"Annots") {
        Ok(Object::Array(items)) => items.len(),
        _ => 0,
    }
}

/// The form XObjects the page content draws, by resource name.
fn drawn_forms(doc: &Document) -> Vec<ObjectId> {
    let page_id = doc.get_pages().get(&1).copied().unwrap();
    let page = doc.get_dictionary(page_id).unwrap();
    let resources = match page.get(b"Resources").unwrap() {
        Object::Reference(id) => doc.get_dictionary(*id).unwrap().clone(),
        Object::Dictionary(dict) => dict.clone(),
        _ => panic!("resources"),
    };
    let xobjects = match resources.get(b"XObject") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).unwrap().clone(),
        Ok(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    };
    xobjects
        .iter()
        .filter(|(name, _)| name.starts_with(b"FA"))
        .filter_map(|(_, value)| value.as_reference().ok())
        .collect()
}

#[test]
fn markup_without_an_appearance_is_drawn_and_burned_in() {
    let (report, flat) = flatten(&mut doc_with(vec![highlight(), square()]));
    assert_eq!(report.annotations_flattened, 2, "{:?}", report.warnings);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(page_annots(&flat), 0, "both annotations are removed once burned in");

    let forms = drawn_forms(&flat);
    assert_eq!(forms.len(), 2);
    for id in forms {
        let stream = flat.get_object(id).unwrap().as_stream().unwrap();
        assert!(!stream.content.is_empty(), "the synthesized form draws something");
    }
    // The synthesized content is already in page space: placed with the identity.
    let page_id = flat.get_pages().get(&1).copied().unwrap();
    let content = String::from_utf8_lossy(&flat.get_page_content(page_id)).to_string();
    assert_eq!(content.matches("1.0000 0.0000 0.0000 1.0000 0.0000 0.0000 cm").count(), 2, "{content}");
}

#[test]
fn a_note_without_an_appearance_or_a_broken_one_is_left_with_a_warning() {
    let note = dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "F" => 4,
        "Rect" => reals(&[50.0, 50.0, 70.0, 70.0]),
        "Contents" => Object::string_literal("kept"),
    };
    // An /AP that does not resolve is never guessed at from the geometry.
    let mut broken = square();
    broken.set("AP", dictionary! { "N" => Object::Reference((9999, 0)) });
    let (report, flat) = flatten(&mut doc_with(vec![note, broken]));
    assert_eq!(report.annotations_flattened, 0);
    assert_eq!(report.warnings.len(), 2, "{:?}", report.warnings);
    assert_eq!(page_annots(&flat), 2);
}

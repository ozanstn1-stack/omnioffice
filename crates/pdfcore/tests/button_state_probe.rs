//! What filling a checkbox writes into the file.
//!
//! A button widget shows one of the appearance states inside `/AP /N`, chosen by
//! `/AS` (ISO 32000-1 12.5.5, Table 168), and the field's value is `/V` (12.7.4.2.3).
//! The two have to agree, or a reader shows a state the document does not hold - and
//! the *name* of the on state is the document's to choose: `/Yes` is a convention, not
//! a rule.

use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdfcore::forms::{fill_fields, FieldValue};

fn ap(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => Object::Array(vec![0.into(), 0.into(), 14.into(), 14.into()]),
            "Resources" => dictionary! {},
        },
        Vec::new(),
    ))))
}

/// A one-page document with a single merged checkbox widget whose on state is
/// `on_state`.
fn checkbox(on_state: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]),
    });
    let pages_id = doc.new_object_id();
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => Object::Integer(1) }));
    let off = ap(&mut doc);
    let on = ap(&mut doc);
    let field = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "T" => Object::String(b"agree".to_vec(), StringFormat::Literal),
        "V" => Object::Name(b"Off".to_vec()),
        "Rect" => Object::Array(vec![72.into(), 540.into(), 86.into(), 554.into()]),
        "P" => Object::Reference(page_id),
        "AP" => dictionary! {
            "N" => dictionary! {
                "Off" => off,
                on_state => on,
            },
        },
    });
    doc.get_object_mut(page_id).and_then(Object::as_dict_mut).expect("page").set("Parent", Object::Reference(pages_id));
    doc.get_object_mut(pages_id)
        .and_then(Object::as_dict_mut)
        .expect("pages")
        .set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let form = doc.add_object(dictionary! { "Fields" => Object::Array(vec![Object::Reference(field)]) });
    let root = catalog;
    doc.get_object_mut(root).and_then(Object::as_dict_mut).expect("catalog").set("AcroForm", Object::Reference(form));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// The `/V` and `/AS` of the one widget, read back from a filled document.
fn written(pdf: &[u8]) -> (Option<String>, Option<String>) {
    let doc = Document::load_mem(pdf).expect("reload");
    let catalog = doc.catalog().expect("catalog");
    let form = catalog.get(b"AcroForm").and_then(Object::as_reference).expect("acroform");
    let field = doc
        .get_dictionary(form)
        .expect("form")
        .get(b"Fields")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|items| items.first())
        .and_then(|value| value.as_reference().ok())
        .expect("field");
    let widget = doc.get_dictionary(field).expect("widget");
    let name = |key: &[u8]| {
        widget
            .get(key)
            .ok()
            .and_then(|value| value.as_name().ok())
            .map(|name| String::from_utf8_lossy(name).into_owned())
    };
    (name(b"V"), name(b"AS"))
}

fn fill(pdf: &[u8], value: &str) -> pdfcore::PdfResult<Vec<u8>> {
    fill_fields(pdf, &[FieldValue { name: "agree".into(), value: value.into(), values: Vec::new() }])
}

#[test]
fn a_checkbox_takes_the_on_state_the_document_declares() {
    // The document calls its on state /Ja; nothing says it has to be /Yes.
    let out = fill(&checkbox("Ja"), "true").expect("fill");
    let (value, appearance) = written(&out);
    assert_eq!(value.as_deref(), Some("Ja"), "the value is the declared on state");
    assert_eq!(appearance.as_deref(), Some("Ja"), "the appearance matches the value");
}

#[test]
fn a_checkbox_with_a_yes_state_still_works() {
    let out = fill(&checkbox("Yes"), "true").expect("fill");
    let (value, appearance) = written(&out);
    assert_eq!(value.as_deref(), Some("Yes"));
    assert_eq!(appearance.as_deref(), Some("Yes"));
}

#[test]
fn clearing_a_checkbox_writes_off_on_both() {
    let out = fill(&checkbox("Ja"), "false").expect("fill");
    let (value, appearance) = written(&out);
    assert_eq!(value.as_deref(), Some("Off"));
    assert_eq!(appearance.as_deref(), Some("Off"), "the appearance is off too");
}

#[test]
fn an_unknown_state_is_refused_and_the_document_is_left_alone() {
    // Writing `/AS /Z` when no appearance stream is named Z shows a blank control,
    // and `/V` would claim a state the file does not have.
    let pdf = checkbox("Ja");
    assert!(fill(&pdf, "Z").is_err(), "an unknown state is refused, not written");
}

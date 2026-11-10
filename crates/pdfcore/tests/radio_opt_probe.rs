//! What filling a radio group writes when the group names its options with `/Opt`.
//!
//! A radio group keeps its value on the field (`/V`, ISO 32000-1 12.7.4.2.4) and each
//! child shows one appearance state through `/AS` (12.5.5, Table 168). `/Opt` gives the
//! options the names a form designer shows (Table 227) - the states themselves stay
//! whatever the document called them. Filling by the printed name has to end up at the
//! state behind it.

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

/// A one-page document with a radio group of `states.len()` children. Child `i` shows
/// `states[i]`; `options` become `/Opt` when non-empty.
fn radio(states: &[&str], options: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));

    let mut kids = Vec::new();
    for state in states {
        let off = ap(&mut doc);
        let on = ap(&mut doc);
        let widget = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => Object::Array(vec![72.into(), 500.into(), 86.into(), 514.into()]),
            "P" => Object::Reference(page_id),
            "AP" => dictionary! {
                "N" => dictionary! {
                    "Off" => off,
                    *state => on,
                },
            },
        });
        kids.push(Object::Reference(widget));
    }
    let mut field = dictionary! {
        "FT" => "Btn",
        "Ff" => Object::Integer(1 << 15),
        "T" => Object::String(b"secim".to_vec(), StringFormat::Literal),
        "V" => Object::Name(b"Off".to_vec()),
        "Kids" => Object::Array(kids),
    };
    if !options.is_empty() {
        field.set(
            "Opt",
            Object::Array(
                options.iter().map(|label| Object::String(label.as_bytes().to_vec(), StringFormat::Literal)).collect(),
            ),
        );
    }
    let field_id = doc.add_object(field);
    let form = doc.add_object(dictionary! { "Fields" => Object::Array(vec![Object::Reference(field_id)]) });
    let root = catalog;
    doc.get_object_mut(root).and_then(Object::as_dict_mut).expect("catalog").set("AcroForm", Object::Reference(form));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// The field's `/V` and each child's `/AS`, read back from a filled document.
fn written(pdf: &[u8]) -> (Option<String>, Vec<Option<String>>) {
    let doc = Document::load_mem(pdf).expect("reload");
    let form = doc.catalog().expect("catalog").get(b"AcroForm").and_then(Object::as_reference).expect("acroform");
    let field = doc
        .get_dictionary(form)
        .expect("form")
        .get(b"Fields")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|items| items.first())
        .and_then(|value| value.as_reference().ok())
        .expect("field");
    let dict = doc.get_dictionary(field).expect("field dict");
    let name = |dict: &lopdf::Dictionary, key: &[u8]| {
        dict.get(key).ok().and_then(|value| value.as_name().ok()).map(|name| String::from_utf8_lossy(name).into_owned())
    };
    let kids = dict.get(b"Kids").and_then(Object::as_array).cloned().unwrap_or_default();
    let states = kids
        .iter()
        .map(|kid| {
            let id = kid.as_reference().expect("kid");
            name(doc.get_dictionary(id).expect("widget"), b"AS")
        })
        .collect();
    (name(dict, b"V"), states)
}

fn fill(pdf: &[u8], value: &str) -> pdfcore::PdfResult<Vec<u8>> {
    fill_fields(pdf, &[FieldValue { name: "secim".into(), value: value.into(), values: Vec::new() }])
}

#[test]
fn a_radio_group_writes_the_value_once_and_the_state_on_each_child() {
    // Three children with states A, B, C; picking B leaves the others off.
    let out = fill(&radio(&["A", "B", "C"], &[]), "B").expect("fill");
    let (value, states) = written(&out);
    assert_eq!(value.as_deref(), Some("B"), "the value lives on the field");
    assert_eq!(
        states,
        vec![Some("Off".into()), Some("B".into()), Some("Off".into())],
        "each child shows its own state"
    );
}

#[test]
fn a_radio_group_can_be_filled_by_the_printed_name() {
    // `/Opt` names the options Elma and Armut while the states stay /0 and /1: a form
    // filled by the printed name has to land on the state behind it (Table 227).
    let out = fill(&radio(&["0", "1"], &["Elma", "Armut"]), "Armut").expect("fill");
    let (value, states) = written(&out);
    assert_eq!(value.as_deref(), Some("1"), "the state behind the printed name");
    assert_eq!(states, vec![Some("Off".into()), Some("1".into())], "the second child is on");
}

#[test]
fn clearing_a_radio_group_turns_every_child_off() {
    let out = fill(&radio(&["A", "B"], &[]), "false").expect("fill");
    let (value, states) = written(&out);
    assert_eq!(value.as_deref(), Some("Off"));
    assert_eq!(states, vec![Some("Off".into()), Some("Off".into())]);
}

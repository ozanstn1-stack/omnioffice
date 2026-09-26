//! End-to-end sanitizer tests.
//!
//! The fixture deliberately hides each hazard in a different place - a name
//! tree, the catalog, a page's additional actions, an annotation - because a
//! sanitizer that only cleans the catalog passes a shallow check and leaves
//! the file dangerous.

mod common;

use common::*;
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdfcore::progress::CancelToken;
use pdfcore::sanitize::{sanitize_pdf, SanitizeOptions};

fn catalog_id(doc: &Document) -> lopdf::ObjectId {
    doc.trailer
        .get(b"Root")
        .expect("catalog")
        .as_reference()
        .expect("catalog reference")
}

fn add_names_entry(doc: &mut Document, key: &str, value: Object) {
    let catalog_id = catalog_id(doc);
    let existing = doc
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"Names").ok())
        .cloned();
    let mut names = match existing {
        Some(Object::Reference(id)) => doc.get_dictionary(id).cloned().unwrap_or_default(),
        Some(Object::Dictionary(dict)) => dict,
        _ => Dictionary::new(),
    };
    names.set(key, value);
    let names_id = doc.add_object(Object::Dictionary(names));
    doc.get_object_mut(catalog_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Names", Object::Reference(names_id));
}

fn page_id(doc: &Document, number: u32) -> lopdf::ObjectId {
    doc.get_pages().get(&number).copied().expect("page exists")
}

/// A two-page document carrying JavaScript, an OpenAction, a page /AA, an
/// embedded file, a FileAttachment annotation and a URI link.
fn poisoned_doc() -> Document {
    let mut doc = build_text_doc(2, "Sanitize", "Poisoned document");

    let js_action = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Action",
        "S" => "JavaScript",
        "JS" => Object::String(b"app.alert('boom')".to_vec(), StringFormat::Literal),
    }));
    let js_tree = doc.add_object(Object::Dictionary(dictionary! {
        "Names" => vec![
            Object::String(b"js1".to_vec(), StringFormat::Literal),
            Object::Reference(js_action),
        ],
    }));
    add_names_entry(&mut doc, "JavaScript", Object::Reference(js_tree));

    let catalog_id = catalog_id(&doc);
    doc.get_object_mut(catalog_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("OpenAction", Object::Reference(js_action));

    let payload = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"payload".to_vec())));
    let filespec = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Filespec",
        "F" => Object::String(b"payload.bin".to_vec(), StringFormat::Literal),
        "EF" => Object::Dictionary(dictionary! {
            "F" => Object::Reference(payload),
        }),
    }));
    let files_tree = doc.add_object(Object::Dictionary(dictionary! {
        "Names" => vec![
            Object::String(b"f1".to_vec(), StringFormat::Literal),
            Object::Reference(filespec),
        ],
    }));
    add_names_entry(&mut doc, "EmbeddedFiles", Object::Reference(files_tree));

    let first_page = page_id(&doc, 1);
    let attachment = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "FileAttachment",
        "Rect" => vec![Object::Real(10.0), Object::Real(10.0), Object::Real(80.0), Object::Real(40.0)],
        "FS" => Object::Reference(filespec),
    }));
    doc.get_object_mut(first_page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(attachment)]));

    let second_page = page_id(&doc, 2);
    let uri_action = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Action",
        "S" => "URI",
        "URI" => Object::String(b"https://example.com".to_vec(), StringFormat::Literal),
    }));
    let link = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![Object::Real(10.0), Object::Real(500.0), Object::Real(200.0), Object::Real(520.0)],
        "A" => Object::Reference(uri_action),
    }));
    doc.get_object_mut(second_page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Annots", Object::Array(vec![Object::Reference(link)]));
    let additional_actions = doc.add_object(Object::Dictionary(dictionary! {
        "O" => Object::Reference(js_action),
    }));
    doc.get_object_mut(second_page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("AA", Object::Reference(additional_actions));

    let xmp = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>".to_vec(),
    )));
    doc.get_object_mut(catalog_id)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Metadata", Object::Reference(xmp));

    doc
}

fn collect(doc: &Document) -> (Vec<String>, Vec<String>) {
    fn walk(value: &Object, keys: &mut Vec<String>, names: &mut Vec<String>) {
        match value {
            Object::Dictionary(dict) => walk_dict(dict, keys, names),
            Object::Stream(stream) => walk_dict(&stream.dict, keys, names),
            Object::Array(items) => {
                for item in items {
                    walk(item, keys, names);
                }
            }
            Object::Name(name) => names.push(String::from_utf8_lossy(name).to_string()),
            _ => {}
        }
    }
    fn walk_dict(dict: &Dictionary, keys: &mut Vec<String>, names: &mut Vec<String>) {
        for (key, value) in dict.iter() {
            keys.push(String::from_utf8_lossy(key).to_string());
            walk(value, keys, names);
        }
    }
    let mut keys = Vec::new();
    let mut names = Vec::new();
    for object in doc.objects.values() {
        walk(object, &mut keys, &mut names);
    }
    (keys, names)
}

fn has_filespec(doc: &Document) -> bool {
    doc.objects.values().any(|object| {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &stream.dict,
            _ => return false,
        };
        dict.has_type(b"Filespec")
    })
}

fn annotation_subtypes(doc: &Document) -> Vec<String> {
    doc.objects
        .values()
        .filter_map(|object| match object {
            Object::Dictionary(dict) => dict.get(b"Subtype").ok()?.as_name().ok(),
            _ => None,
        })
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect()
}

#[test]
fn sanitize_removes_every_hazard_from_every_object() {
    let dir = TestDir::new();
    let source = dir.path("poisoned.pdf");
    write_doc(&mut poisoned_doc(), &source);
    let output = dir.path("clean.pdf");
    let report = sanitize_pdf(
        &source,
        &output,
        &SanitizeOptions::default(),
        &no_progress,
        &CancelToken::new(),
    )
    .expect("sanitize");

    assert!(report.findings_before.javascript_entries > 0, "fixture must carry JS");
    assert!(report.findings_before.embedded_files > 0, "fixture must carry an attachment");
    assert!(report.findings_before.unsafe_annotations > 0, "fixture must carry a FileAttachment");
    assert!(report.findings_before.link_annotations > 0, "fixture must carry a link");
    assert!(report.findings_before.open_action_present, "fixture must carry an OpenAction");
    assert!(report.findings_before.metadata_present, "fixture must carry metadata");
    assert!(report.javascript_removed > 0);
    assert!(report.embedded_files_removed > 0);
    assert!(report.actions_removed > 0);
    assert!(report.metadata_removed > 0);
    assert!(report.annotations_removed > 0);
    assert!(report.links_removed > 0);

    let clean = Document::load(&output).expect("reopen sanitized file");
    let (keys, names) = collect(&clean);
    for banned in ["JS", "JavaScript", "OpenAction", "AA", "EmbeddedFiles", "Metadata"] {
        assert!(
            !keys.iter().any(|key| key == banned),
            "the key /{banned} survived in some object: {keys:?}"
        );
    }
    assert!(
        !names.iter().any(|name| name == "JavaScript" || name == "Filespec"),
        "a hazardous name survived: {names:?}"
    );
    assert!(!has_filespec(&clean), "a file specification object survived");
    let subtypes = annotation_subtypes(&clean);
    for unsafe_subtype in ["FileAttachment", "Sound", "Movie", "Screen", "RichMedia", "Link"] {
        assert!(
            !subtypes.iter().any(|subtype| subtype == unsafe_subtype),
            "the {unsafe_subtype} annotation survived: {subtypes:?}"
        );
    }

    let inspection = pdfcore::inspect::inspect_document(&output, None).expect("inspect");
    assert!(!inspection.has_javascript, "inspector still sees JavaScript");
    assert!(!inspection.has_open_action, "inspector still sees an OpenAction");
    assert!(inspection.embedded_files.is_empty(), "inspector still sees attachments");
    assert!(inspection.title_override.is_empty(), "the Info title survived");
    assert!(inspection.annotations.is_empty(), "an annotation survived");

    assert_eq!(page_count(&output), 2, "sanitizing must not change the page count");
    let text = page_text(&output, 1);
    assert!(text.contains("Sanitize"), "page text must stay extractable: {text:?}");
}

#[test]
fn selective_options_keep_what_was_not_selected() {
    let dir = TestDir::new();
    let source = dir.path("poisoned.pdf");
    write_doc(&mut poisoned_doc(), &source);
    let output = dir.path("clean.pdf");
    let options = SanitizeOptions {
        remove_javascript: false,
        remove_metadata: false,
        ..Default::default()
    };
    sanitize_pdf(&source, &output, &options, &no_progress, &CancelToken::new()).expect("sanitize");
    let clean = Document::load(&output).expect("reopen");
    let (keys, _) = collect(&clean);
    assert!(
        keys.iter().any(|key| key == "JavaScript" || key == "JS"),
        "JavaScript was not selected for removal and must survive: {keys:?}"
    );
    assert!(clean.trailer.get(b"Info").is_ok(), "metadata was not selected for removal");
    assert!(!has_filespec(&clean), "embedded files were still selected for removal");
}

#[test]
fn a_cancelled_sanitize_leaves_no_output() {
    let dir = TestDir::new();
    let source = dir.path("poisoned.pdf");
    write_doc(&mut poisoned_doc(), &source);
    let output = dir.path("cancelled.pdf");
    let token = CancelToken::new();
    token.cancel();
    let error = sanitize_pdf(&source, &output, &SanitizeOptions::default(), &no_progress, &token);
    assert!(error.is_err(), "a cancelled sanitize must report an error");
    assert!(!output.exists(), "a cancelled sanitize must not leave a file behind");
}

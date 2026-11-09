//! Whether a bundle keeps the exhibit's own page working.
//!
//! Stamping a label means adding a resource and appending content to a page that
//! belongs to the user's file. The page may inherit its resources from the page
//! tree (ISO 32000-1 7.7.3.4), define its own font under a name the stamp also
//! wants (7.8.3), share one content stream with another page (7.8.2), or carry
//! compressed content (7.4.4). None of that may break the exhibit's own text.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::evidence::{build_evidence_bundle, EvidenceDetails, EvidenceFile};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn details() -> EvidenceDetails {
    EvidenceDetails {
        title: "Delil".into(),
        case_reference: String::new(),
        court: String::new(),
        submitted_by: String::new(),
        language: "tr".into(),
    }
}

fn done(bundle: &[u8], page: u32) -> (String, Document) {
    let doc = Document::load_mem(bundle).expect("reload");
    let page_id = *doc.get_pages().get(&page).expect("page");
    let text = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    (text, doc)
}

#[test]
fn an_inherited_resource_stays_on_the_page() {
    // The exhibit's font lives on the page tree; after stamping, the page still
    // finds it and the label font sits on the page itself.
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf (exhibit) Tj ET".to_vec()));
    let font = doc.add_object(Object::Dictionary(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" },
    ));
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } });
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(root),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
    });
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Resources" => Object::Reference(resources),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let (bundle, manifest) = build_evidence_bundle(
        vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }],
        &details(),
    )
    .expect("bundle");
    let (text, doc) = done(&bundle, manifest.entries[0].first_page);
    assert!(text.contains("exhibit"), "the exhibit's own text is still there");
    let page_id = *doc.get_pages().get(&manifest.entries[0].first_page).expect("page");
    let page_dict = doc.get_dictionary(page_id).expect("page dict");
    let own = page_dict.get(b"Resources").expect("the page owns its resources after stamping");
    assert!(!matches!(own, Object::Null), "resources are on the page");
    let resources = doc.get_dictionary(own.as_reference().expect("reference")).expect("resources");
    // `/Font` may itself be behind a reference (ISO 32000-1 7.8.3).
    let font_value = resources.get(b"Font").expect("font dictionary");
    let fonts = match font_value {
        Object::Reference(id) => doc.get_dictionary(*id).expect("font dictionary"),
        Object::Dictionary(dict) => dict,
        other => panic!("unexpected /Font: {other:?}"),
    };
    assert!(fonts.has(b"F1"), "the inherited font is kept under its name");
    assert!(fonts.has(b"OmniTr"), "the label font is added");
}

#[test]
fn two_pages_sharing_a_content_stream_each_get_their_own_label() {
    // Both pages point at one stream. Stamping must add a stream per page, not
    // edit the one they share, or the second label would land on both pages.
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let shared = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf (shared) Tj ET".to_vec()));
    let font = doc.add_object(Object::Dictionary(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" },
    ));
    let resources_id = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } });
    let mut kids = Vec::new();
    for _ in 0..2 {
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(root),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Contents" => Object::Reference(shared),
            "Resources" => Object::Reference(resources_id),
        });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(kids),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let (bundle, manifest) = build_evidence_bundle(
        vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }],
        &details(),
    )
    .expect("bundle");
    let first = manifest.entries[0].first_page;
    for page in first..=manifest.entries[0].last_page {
        let (text, _) = done(&bundle, page);
        assert!(text.contains("shared"), "page {page} keeps the shared stream");
        assert_eq!(text.matches("OmniTr").count(), 2, "page {page} gets one label and one footer");
    }
}

#[test]
fn a_compressed_exhibit_keeps_its_text() {
    // The exhibit's content stream is Flate-encoded (7.4.4); the label is added
    // as its own stream, so the original still decompresses to the same text.
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let plain = b"BT /F1 12 Tf (compressed exhibit) Tj ET".to_vec();
    let mut stream = Stream::new(dictionary! {}, plain);
    stream.compress().expect("compress the exhibit content");
    let content = doc.add_object(stream);
    let font = doc.add_object(Object::Dictionary(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" },
    ));
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } });
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(root),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Reference(resources),
    });
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let (bundle, manifest) = build_evidence_bundle(
        vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }],
        &details(),
    )
    .expect("bundle");
    let (text, _) = done(&bundle, manifest.entries[0].first_page);
    assert!(text.contains("compressed exhibit"), "the exhibit's own text survives: {text}");
    assert!(text.contains("OmniTr"), "and the label is drawn");
}

#[test]
fn a_font_name_the_exhibit_already_uses_is_not_reused() {
    // A file this app already stamped carries its own `/OmniTr` font. Stamping a
    // bundle label over that name would render the exhibit's existing text with
    // the label's subset, so the label has to take a name the page does not use
    // (ISO 32000-1 7.8.3).
    let mut doc = Document::with_version("1.7");
    let root = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /OmniTr 12 Tf (old stamp) Tj ET".to_vec()));
    let old_font = doc.add_object(Object::Dictionary(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier" },
    ));
    let resources = doc.add_object(dictionary! { "Font" => dictionary! { "OmniTr" => Object::Reference(old_font) } });
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(root),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Reference(resources),
    });
    doc.objects.insert(
        root,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");

    let (bundle, manifest) = build_evidence_bundle(
        vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }],
        &details(),
    )
    .expect("bundle");
    let doc = Document::load_mem(&bundle).expect("reload");
    let page_id = *doc.get_pages().get(&manifest.entries[0].first_page).expect("page");
    let page_dict = doc.get_dictionary(page_id).expect("page dict");
    let resources_id = page_dict.get(b"Resources").and_then(Object::as_reference).expect("own resources");
    let resources = doc.get_dictionary(resources_id).expect("resources");
    let font_value = resources.get(b"Font").expect("font dictionary");
    let fonts = match font_value {
        Object::Reference(id) => doc.get_dictionary(*id).expect("font dictionary"),
        Object::Dictionary(dict) => dict,
        other => panic!("unexpected /Font: {other:?}"),
    };
    // The bundle renumbers objects while it merges the exhibit in, so the check
    // is on what the names resolve to: the exhibit's own name keeps a plain
    // Courier, and the label takes a second name instead of taking that one over.
    let original = fonts.get(b"OmniTr").expect("the exhibit's font name survives");
    let label = fonts.get(b"OmniTr2").expect("the label takes a free name");
    assert_ne!(original, label, "the label does not reuse the exhibit's font name");
    let original_id = original.as_reference().expect("a reference");
    let original_font = doc.get_dictionary(original_id).expect("the exhibit's font");
    let base = original_font.get(b"BaseFont").and_then(Object::as_name).unwrap_or(b"");
    assert_eq!(base, b"Courier", "the exhibit keeps the font its text was drawn with");
}

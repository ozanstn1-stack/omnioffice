//! How an evidence bundle lays its exhibits out: numbering, order and refusal.
//!
//! The manifest is what a reader trusts to find an exhibit again, so its page
//! numbers have to be the bundle's own pages, contiguous and in the order the
//! files were submitted - not the order their objects happen to sit in the file
//! (ISO 32000-1 7.7.3.2). A bundle is one cover page, then index pages with
//! fifteen rows each, then the exhibits in order.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::evidence::{build_evidence_bundle, EvidenceDetails, EvidenceFile};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A `pages`-page PDF whose page content carries `tag`.
fn exhibit(pages: u32, tag: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for index in 0..pages {
        let text = format!("BT /F1 12 Tf 72 720 Td ({tag}-{index}) Tj ET");
        let content_id = doc.add_object(Stream::new(dictionary! {}, text.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Contents" => Object::Reference(content_id),
        });
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => pages as i64, "Kids" => kids }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// A two-page PDF whose page tree has an intermediate node, and whose page
/// objects are created in the opposite order to the one they are listed in.
fn nested_exhibit() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let root_id = doc.new_object_id();
    let inner_id = doc.new_object_id();
    let second_content = doc.add_object(Stream::new(dictionary! {}, b"BT (second-1) Tj ET".to_vec()));
    let second = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(inner_id),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(second_content),
    });
    let first_content = doc.add_object(Stream::new(dictionary! {}, b"BT (first-0) Tj ET".to_vec()));
    let first = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(inner_id),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(first_content),
    });
    doc.objects.insert(
        inner_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Parent" => Object::Reference(root_id),
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(vec![Object::Reference(first), Object::Reference(second)]),
        }),
    );
    doc.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(2),
            "Kids" => Object::Array(vec![Object::Reference(inner_id)]),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(root_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
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

fn file(label: &str, bytes: Vec<u8>) -> EvidenceFile {
    EvidenceFile { label: label.into(), file_name: format!("{label}.pdf"), bytes }
}

fn page_text(bundle: &[u8], page: u32) -> String {
    let doc = Document::load_mem(bundle).expect("reload the bundle");
    let page_id = *doc.get_pages().get(&page).expect("the page");
    String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned()
}

#[test]
fn exhibits_follow_the_input_order_and_tile_the_pages() {
    // One cover, one index page (two rows fit on it), then A's three pages and
    // B's two: 1 + 1 + 5 = 7 pages, A on 3..5 and B on 6..7.
    let files = vec![file("A", exhibit(3, "a")), file("B", exhibit(2, "b"))];
    let (bundle, manifest) = build_evidence_bundle(files, &details()).expect("bundle");
    assert_eq!(manifest.total_pages, 7, "cover + index + five exhibit pages");
    let ranges: Vec<(u32, u32, &str)> =
        manifest.entries.iter().map(|entry| (entry.first_page, entry.last_page, entry.label.as_str())).collect();
    assert_eq!(ranges, vec![(3, 5, "A"), (6, 7, "B")], "the ranges tile the pages in input order");
    assert_eq!(manifest.entries[0].pages, 3);
    assert_eq!(manifest.entries[1].pages, 2);
    assert!(page_text(&bundle, 3).contains("a-0"), "the first exhibit starts after the front matter");
    assert!(page_text(&bundle, 6).contains("b-0"), "the second exhibit follows the first");
}

#[test]
fn the_manifest_follows_the_submission_order_not_the_tag_order() {
    let files = vec![file("B", exhibit(2, "b")), file("A", exhibit(3, "a"))];
    let (bundle, manifest) = build_evidence_bundle(files, &details()).expect("bundle");
    let labels: Vec<&str> = manifest.entries.iter().map(|entry| entry.label.as_str()).collect();
    assert_eq!(labels, vec!["B", "A"], "the order the files were given is the order they appear");
    assert_eq!((manifest.entries[0].first_page, manifest.entries[0].last_page), (3, 4));
    assert_eq!((manifest.entries[1].first_page, manifest.entries[1].last_page), (5, 7));
    assert!(page_text(&bundle, 3).contains("b-0"));
    assert!(page_text(&bundle, 5).contains("a-0"));
}

#[test]
fn a_nested_page_tree_keeps_its_reading_order() {
    // The page objects are created in the opposite order to the one the tree
    // lists them in; the bundle has to follow the tree (ISO 32000-1 7.7.3.2).
    let (bundle, manifest) = build_evidence_bundle(vec![file("A", nested_exhibit())], &details()).expect("bundle");
    assert_eq!(manifest.entries[0].pages, 2);
    let first = manifest.entries[0].first_page;
    assert!(page_text(&bundle, first).contains("first-0"), "the tree's first kid comes first");
    assert!(page_text(&bundle, first + 1).contains("second-1"), "the tree's second kid comes second");
}

#[test]
fn an_empty_exhibit_is_refused_without_writing_a_bundle() {
    let files = vec![file("A", exhibit(1, "a")), file("B", Vec::new())];
    assert!(build_evidence_bundle(files, &details()).is_err(), "a zero-byte exhibit is not a document");
}

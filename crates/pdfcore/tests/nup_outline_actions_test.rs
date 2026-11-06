//! Outline entries that carry actions, sit in a booklet sheet or arrive broken.

#[path = "common/outline_check.rs"]
mod check;
#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/outline_fixtures.rs"]
mod outlines;

use check::check_outline;
use fixtures::{impose, page_content, paint_matrices, sheets, PageSpec, TestDir};

const TOLERANCE: f64 = 0.05;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use outlines::{add_outline, dest_of, integer, top_items, OutlineSpec};
use pdfcore::nup::NupOptions;

/// `count` default pages.
fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

#[test]
fn uri_entries_stay_and_unsafe_actions_go() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("uri.pdf");
    fixture.write(&input);
    let outline = [
        OutlineSpec::new("Website", Some(outlines::uri_action("https://example.com/doc"))),
        OutlineSpec::new(
            "Script",
            Some(Object::Dictionary(dictionary! {
                "S" => "JavaScript",
                "JS" => Object::string_literal("app.alert(1)"),
            })),
        ),
    ];
    add_outline(&input, &outline, None);

    let out = dir.path("uri-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let items = top_items(&doc);
    assert_eq!(items.len(), 1, "the script entry is dropped, the URI entry stays");
    let action = items[0].get(b"A").expect("action").as_dict().expect("action dict");
    assert_eq!(action.get(b"URI").unwrap().as_str().unwrap(), b"https://example.com/doc");
    check_outline(&doc);
}

#[test]
fn booklet_puts_an_entry_on_the_sheet_that_carries_its_page() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(4));
    let input = dir.path("booklet.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [OutlineSpec::new("Third page", Some(outlines::xyz(pages[2], 10.0, 700.0, None)))];
    add_outline(&input, &outline, None);

    let out = dir.path("booklet-out.pdf");
    impose(&input, &out, &NupOptions { booklet: true, ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc);
    assert_eq!(sheet.len(), 2);
    let items = top_items(&doc);
    let (page, view, numbers) = dest_of(&doc, &items[0]).expect("destination");
    assert_eq!(page, sheet[1].1, "page three rides on the second booklet sheet");
    assert_eq!(view.as_slice(), b"XYZ");
    let matrices = paint_matrices(&page_content(&doc, sheet[1].1));
    let expected = fixtures::apply(&matrices[1], 10.0, 700.0);
    assert!((numbers[0] - expected.0).abs() <= TOLERANCE);
    assert!((numbers[1] - expected.1).abs() <= TOLERANCE);
}

#[test]
fn style_and_title_bytes_survive() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("style.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [OutlineSpec::new("Bölüm Başlığı", Some(outlines::xyz(pages[0], 10.0, 10.0, None)))
        .colored([1.0, 0.0, 0.0])
        .italic()];
    add_outline(&input, &outline, None);

    let out = dir.path("style-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let items = top_items(&doc);
    assert_eq!(items[0].get(b"Title").unwrap().as_str().unwrap(), "Bölüm Başlığı".as_bytes());
    assert_eq!(items[0].get(b"C").unwrap().as_array().unwrap().len(), 3);
    assert_eq!(integer(&items[0], "F"), Some(1));
}

#[test]
fn broken_entries_are_tolerated() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("broken.pdf");
    fixture.write(&input);
    // Hand-built outline: one item without a title, one whose /First is not a
    // dictionary, and a valid sibling after them.
    let mut doc = Document::load(&input).expect("load input");
    let catalog = outlines::catalog_id(&doc);
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let root = doc.new_object_id();
    let titleless = doc.new_object_id();
    let bad_first = doc.new_object_id();
    let valid = doc.new_object_id();
    let mut first_dict = Dictionary::new();
    first_dict.set("Subtype", Object::Name(b"NotAnItem".to_vec()));
    first_dict.set("Next", Object::Reference(bad_first));
    doc.objects.insert(titleless, Object::Dictionary(first_dict));
    let mut bad = Dictionary::new();
    bad.set("Title", Object::string_literal("Broken children"));
    bad.set("Parent", Object::Reference(root));
    bad.set("First", Object::Integer(5));
    bad.set("Next", Object::Reference(valid));
    bad.set("Prev", Object::Reference(titleless));
    doc.objects.insert(bad_first, Object::Dictionary(bad));
    let mut last = Dictionary::new();
    last.set("Title", Object::string_literal("Valid"));
    last.set("Parent", Object::Reference(root));
    last.set("Prev", Object::Reference(bad_first));
    last.set("Dest", outlines::xyz(pages[0], 10.0, 10.0, None));
    doc.objects.insert(valid, Object::Dictionary(last));
    let mut root_dict = Dictionary::new();
    root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
    root_dict.set("First", Object::Reference(titleless));
    root_dict.set("Last", Object::Reference(valid));
    root_dict.set("Count", Object::Integer(2));
    doc.objects.insert(root, Object::Dictionary(root_dict));
    doc.get_dictionary_mut(catalog).expect("catalog").set("Outlines", Object::Reference(root));
    doc.save(&input).expect("save");

    let out = dir.path("broken-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("broken outline still imposes");
    let items = top_items(&doc);
    assert_eq!(items.len(), 1, "the title-less item is dropped, the valid one survives");
    assert_eq!(items[0].get(b"Title").unwrap().as_str().unwrap(), b"Valid");
    check_outline(&doc);
}

#[test]
fn a_next_cycle_terminates() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("cycle.pdf");
    fixture.write(&input);
    let mut doc = Document::load(&input).expect("load input");
    let catalog = outlines::catalog_id(&doc);
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let root = doc.new_object_id();
    let item = doc.new_object_id();
    let mut dict = Dictionary::new();
    dict.set("Title", Object::string_literal("Loops"));
    dict.set("Parent", Object::Reference(root));
    dict.set("Next", Object::Reference(item));
    dict.set("Dest", outlines::xyz(pages[0], 10.0, 10.0, None));
    doc.objects.insert(item, Object::Dictionary(dict));
    let mut root_dict = Dictionary::new();
    root_dict.set("Type", Object::Name(b"Outlines".to_vec()));
    root_dict.set("First", Object::Reference(item));
    root_dict.set("Last", Object::Reference(item));
    root_dict.set("Count", Object::Integer(1));
    doc.objects.insert(root, Object::Dictionary(root_dict));
    doc.get_dictionary_mut(catalog).expect("catalog").set("Outlines", Object::Reference(root));
    doc.save(&input).expect("save");

    let out = dir.path("cycle-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("a looping chain must not hang the imposition");
    let items = top_items(&doc);
    assert_eq!(items.len(), 1, "the loop is cut after the first item");
    check_outline(&doc);
}

#[test]
fn the_outline_survives_a_save_and_reload() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(4));
    let input = dir.path("round.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [
        OutlineSpec::new("One", Some(outlines::view(pages[0], "Fit"))),
        OutlineSpec::new("Two", Some(outlines::xyz(pages[2], 5.0, 5.0, None))),
    ];
    add_outline(&input, &outline, Some("UseOutlines"));

    let out = dir.path("round-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let mut first = Document::load(&out).expect("load output");
    let (tops, total) = check_outline(&first);
    assert_eq!((tops, total), (2, 2));
    let rewritten = dir.path("round-again.pdf");
    first.save(&rewritten).expect("save again");
    let again = Document::load(&rewritten).expect("reload");
    let (tops, total) = check_outline(&again);
    assert_eq!((tops, total), (2, 2));
    assert_eq!(top_items(&again).len(), 2);
}

//! Bookmarks survive N-up and booklet imposition: the outline is rebuilt on the
//! sheet pages, and entries that point nowhere disappear.

#[path = "common/outline_check.rs"]
mod check;
#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/outline_fixtures.rs"]
mod outlines;

use check::check_outline;
use fixtures::{close, impose, page_content, paint_matrices, placed_rect, sheets, PageSpec, TestDir};
use lopdf::{Document, Object, ObjectId};
use outlines::{add_outline, dest_of, integer, outline_root, top_items, OutlineSpec};
use pdfcore::nup::NupOptions;

const PAGE_W: f64 = 600.0;
const PAGE_H: f64 = 800.0;
const TOLERANCE: f64 = 0.05;

/// `count` default pages.
fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

#[test]
fn a_flat_outline_lands_on_the_sheets() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("flat.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [
        OutlineSpec::new("First page", Some(outlines::xyz(pages[0], 40.0, 700.0, None))),
        OutlineSpec::new("Second page", Some(outlines::view(pages[1], "Fit"))),
    ];
    add_outline(&input, &outline, Some("UseOutlines"));

    let out = dir.path("flat-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let (tops, total) = check_outline(&doc);
    assert_eq!(tops, 2, "both entries survive");
    assert_eq!(total, 2);
    let items = top_items(&doc);
    assert_eq!(items.len(), 2);

    let sheet = sheets(&doc);
    assert_eq!(sheet.len(), 1, "two pages make one 2-up sheet");
    let matrices = paint_matrices(&page_content(&doc, sheet[0].1));

    let (page, view, numbers) = dest_of(&doc, &items[0]).expect("destination");
    assert_eq!(page, sheet[0].1, "the entry points at the sheet page");
    assert_eq!(view.as_slice(), b"XYZ");
    let expected = fixtures::apply(&matrices[0], 40.0, 700.0);
    assert!((numbers[0] - expected.0).abs() <= TOLERANCE, "{numbers:?} != {expected:?}");
    assert!((numbers[1] - expected.1).abs() <= TOLERANCE);

    let (page, view, numbers) = dest_of(&doc, &items[1]).expect("destination");
    assert_eq!(page, sheet[0].1);
    assert_eq!(view.as_slice(), b"FitR", "a page-level view becomes the cell rectangle");
    let cell = placed_rect(&matrices[1], PAGE_W, PAGE_H);
    assert!(close([numbers[0], numbers[1], numbers[2], numbers[3]], cell, TOLERANCE));

    assert_eq!(doc.catalog().unwrap().get(b"PageMode").unwrap().as_name().unwrap(), b"UseOutlines");
}

#[test]
fn nested_entries_keep_their_counts() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(3));
    let input = dir.path("nested.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [
        OutlineSpec::new("Open chapter", Some(outlines::xyz(pages[0], 10.0, 10.0, None)))
            .open()
            .child(OutlineSpec::new("Section", Some(outlines::xyz(pages[1], 20.0, 20.0, None))))
            .child(OutlineSpec::new("Section 2", Some(outlines::xyz(pages[1], 30.0, 30.0, None)))),
        OutlineSpec::new("Closed chapter", Some(outlines::view(pages[2], "Fit")))
            .child(OutlineSpec::new("Hidden section", Some(outlines::xyz(pages[2], 40.0, 40.0, None)))),
    ];
    add_outline(&input, &outline, None);

    let out = dir.path("nested-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let (_, total) = check_outline(&doc);
    assert_eq!(total, 5, "two chapters, two visible sections and one hidden one");
    let items = top_items(&doc);
    assert_eq!(integer(&items[0], "Count"), Some(2), "an open chapter is positive");
    assert_eq!(integer(&items[1], "Count"), Some(-1), "a closed chapter counts its subtree negative");
    let children = outlines::children_of(&doc, &items[0]);
    assert_eq!(children.len(), 2);
}

#[test]
fn a_dropped_leaf_keeps_its_heading_parent() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(4));
    let input = dir.path("trim.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [
        // Points outside the selection and carries no children: dropped.
        OutlineSpec::new("Unselected", Some(outlines::xyz(pages[3], 10.0, 10.0, None))),
        // A heading with one surviving child and one outside the selection: the
        // heading stays (without a target) and keeps the child that made it.
        OutlineSpec::new("Heading", None)
            .open()
            .child(OutlineSpec::new("Kept child", Some(outlines::xyz(pages[1], 30.0, 30.0, None))))
            .child(OutlineSpec::new("Unselected child", Some(outlines::xyz(pages[3], 20.0, 20.0, None)))),
    ];
    add_outline(&input, &outline, Some("UseOutlines"));

    let out = dir.path("trim-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1, 2], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let (tops, total) = check_outline(&doc);
    assert_eq!(tops, 1, "only the heading survives");
    assert_eq!(total, 2, "the heading and the child that could be mapped");
    let items = top_items(&doc);
    assert!(items[0].get(b"Dest").is_err() && items[0].get(b"A").is_err(), "the heading has no target");
    let children = outlines::children_of(&doc, &items[0]);
    assert_eq!(children.len(), 1, "the unselected child is gone");
    assert!(children[0].get(b"Dest").is_ok(), "the kept child points at a sheet");
    assert_eq!(doc.catalog().unwrap().get(b"PageMode").unwrap().as_name().unwrap(), b"UseOutlines");
}

#[test]
fn an_outline_with_no_surviving_target_does_not_appear() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(3));
    let input = dir.path("gone.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline = [
        OutlineSpec::new("Away", Some(outlines::xyz(pages[2], 10.0, 10.0, None))),
        OutlineSpec::new("Away too", Some(outlines::view(pages[2], "Fit"))),
    ];
    add_outline(&input, &outline, None);

    let out = dir.path("gone-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    assert!(outline_root(&doc).is_none(), "no entry can reach a sheet, so there is no outline");
    assert!(doc.catalog().unwrap().get(b"PageMode").is_err(), "nothing opens an empty panel");
}

#[test]
fn a_document_without_an_outline_gains_none() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("plain.pdf");
    fixture.write(&input);
    let out = dir.path("plain-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    assert!(outline_root(&doc).is_none());
}

#[test]
fn named_outline_destinations_resolve() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&page_specs(3));
    let input = dir.path("named.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    // The outline refers to the name the catalog's tree carries.
    fixture.named_destinations(&[("chapter", outlines::xyz(pages[1], 60.0, 500.0, None))], None);
    fixture.write(&input);
    let outline = [OutlineSpec::new("Chapter", Some(Object::string_literal("chapter")))];
    add_outline(&input, &outline, None);

    let out = dir.path("named-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let items = top_items(&doc);
    assert_eq!(items.len(), 1);
    let (page, view, _) = dest_of(&doc, &items[0]).expect("the named destination became explicit");
    assert_eq!(page, sheets(&doc)[0].1);
    assert_eq!(view.as_slice(), b"XYZ");
}

#[test]
fn a_goto_action_becomes_a_plain_destination() {
    let dir = TestDir::new();
    let mut fixture = fixtures::Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let input = dir.path("goto.pdf");
    fixture.write(&input);
    let probe = Document::load(&input).expect("load input");
    let pages: Vec<ObjectId> = probe.get_pages().values().copied().collect();
    drop(probe);
    let outline =
        [OutlineSpec::new("Second", Some(outlines::goto_action(outlines::xyz(pages[1], 10.0, 20.0, Some(2)))))];
    add_outline(&input, &outline, None);

    let out = dir.path("goto-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let items = top_items(&doc);
    assert!(items[0].get(b"A").is_err(), "an internal action is normalised to a destination");
    let (_, view, numbers) = dest_of(&doc, &items[0]).expect("destination");
    assert_eq!(view.as_slice(), b"XYZ");
    assert_eq!(numbers.len(), 2, "the source zoom is dropped");
}

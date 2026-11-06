//! Link annotations survive N-up and booklet imposition: rectangles, external
//! targets and internal destinations are mapped onto the sheets.

#[path = "common/link_fixtures.rs"]
mod fixtures;

use fixtures::*;
use lopdf::{Document, Object};
use pdfcore::nup::NupOptions;

const PAGE_W: f64 = 600.0;
const PAGE_H: f64 = 800.0;
/// Destination numbers are stored as 32-bit reals, and the sheet matrix is
/// written with four decimals, so a rebuilt coordinate lands a little off.
const TOLERANCE: f64 = 0.05;

/// The source page rectangle clipped into the cell it was placed in.
fn expected_rect(matrix: &[f64; 6], rect: [f64; 4]) -> [f64; 4] {
    let mapped = map_rect(matrix, rect);
    let cell = placed_rect(matrix, PAGE_W, PAGE_H);
    [mapped[0].max(cell[0]), mapped[1].max(cell[1]), mapped[2].min(cell[2]), mapped[3].min(cell[3])]
}

#[test]
fn uri_link_rect_lands_in_its_cell() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    fixture.annot(1, with_action(link([100.0, 100.0, 200.0, 150.0]), uri_action("https://example.com/a")));
    let input = dir.path("two.pdf");
    fixture.write(&input);

    let out = dir.path("two-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 1, "two pages make one 2-up sheet");
    let sheet_id = sheets[0].1;
    let annots = annots(&doc, sheet_id);
    assert_eq!(annots.len(), 1, "the single link is carried over");

    let matrices = paint_matrices(&page_content(&doc, sheet_id));
    assert_eq!(matrices.len(), 2, "two placements");
    // Slot 1 carries source page 2.
    let expected = expected_rect(&matrices[1], [100.0, 100.0, 200.0, 150.0]);
    let rect = rect_of(annots[0].get(b"Rect").expect("rect")).expect("rect array");
    assert!(close(rect, expected, TOLERANCE), "rect {rect:?} != expected {expected:?}");

    let action = annots[0].get(b"A").expect("action").as_dict().expect("action dict");
    assert_eq!(action.get(b"S").unwrap().as_name().unwrap(), b"URI");
    assert_eq!(action.get(b"URI").unwrap().as_str().unwrap(), b"https://example.com/a");
    assert_eq!(annots[0].get(b"P").unwrap().as_reference().unwrap(), sheet_id);
    assert_eq!(annots[0].get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
    // The cell of the other page stays free of the clickable area.
    let other = placed_rect(&matrices[0], PAGE_W, PAGE_H);
    assert!(overlap(rect, other) == 0.0, "link must not reach into the other cell");
}

#[test]
fn rotated_page_link_rect_is_axis_aligned() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec { rotate: 90, ..PageSpec::default() }]);
    fixture.annot(0, with_action(link([50.0, 60.0, 250.0, 160.0]), uri_action("https://example.com/rot")));
    let input = dir.path("rotated.pdf");
    fixture.write(&input);

    let out = dir.path("rotated-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet_id = sheets(&doc)[0].1;
    let annots = annots(&doc, sheet_id);
    assert_eq!(annots.len(), 1);

    let matrix = paint_matrices(&page_content(&doc, sheet_id))[0];
    // A quarter turn puts the source rectangle on the sheet rotated.
    let expected = expected_rect(&matrix, [50.0, 60.0, 250.0, 160.0]);
    let rect = rect_of(annots[0].get(b"Rect").expect("rect")).expect("rect array");
    assert!(close(rect, expected, TOLERANCE), "rect {rect:?} != expected {expected:?}");
    assert!(rect[2] - rect[0] > 1.0 && rect[3] - rect[1] > 1.0, "the mapped rect must not collapse: {rect:?}");
    assert!(matrix[1] < 0.0 && matrix[2] > 0.0, "the source page is turned clockwise: {matrix:?}");
}

#[test]
fn goto_link_targets_second_sheet() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    let page_three = fixture.page(2);
    let page_four = fixture.page(3);
    fixture.annot(
        0,
        with_action(
            link([40.0, 40.0, 140.0, 90.0]),
            goto_action(xyz(page_four, real(50.0), real(700.0), Object::Null)),
        ),
    );
    fixture.annot(0, with_dest(link([40.0, 500.0, 140.0, 550.0]), xyz(page_three, real(10.0), real(20.0), integer(2))));
    let input = dir.path("four.pdf");
    fixture.write(&input);

    let out = dir.path("four-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 2, "four pages make two 2-up sheets");
    let first = sheets[0].1;
    let second = sheets[1].1;
    let annots = annots(&doc, first);
    assert_eq!(annots.len(), 2, "both internal links are carried over");
    let second_matrices = paint_matrices(&page_content(&doc, second));

    // Page 4 sits in the second slot of the second sheet.
    let dest = dest_of(&annots[0]);
    assert_eq!(dest.first().unwrap().as_reference().unwrap(), second, "GoTo must point at the sheet page");
    assert_eq!(dest[1].as_name().unwrap(), b"XYZ");
    let expected = apply(&second_matrices[1], 50.0, 700.0);
    assert!((number(&dest[2]).unwrap() - expected.0).abs() <= TOLERANCE, "x {dest:?} != {expected:?}");
    assert!((number(&dest[3]).unwrap() - expected.1).abs() <= TOLERANCE, "y {dest:?} != {expected:?}");
    assert!(matches!(dest[4], Object::Null), "the source zoom is dropped");
    assert!(annots[0].get(b"A").is_err(), "an internal link carries /Dest, not /A");

    // Page 3 is the first slot of the second sheet.
    let dest = dest_of(&annots[1]);
    assert_eq!(dest.first().unwrap().as_reference().unwrap(), second);
    let expected = apply(&second_matrices[0], 10.0, 20.0);
    assert!((number(&dest[2]).unwrap() - expected.0).abs() <= TOLERANCE);
    assert!((number(&dest[3]).unwrap() - expected.1).abs() <= TOLERANCE);
}

#[test]
fn fit_family_becomes_fitr_of_the_target_cell() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    let page_two = fixture.page(1);
    fixture.annot(0, with_action(link([10.0, 10.0, 60.0, 60.0]), goto_action(fit(page_two))));
    fixture.annot(0, with_action(link([10.0, 70.0, 60.0, 120.0]), goto_action(fit_h(page_two, real(500.0)))));
    fixture.annot(
        0,
        with_action(
            link([10.0, 130.0, 60.0, 180.0]),
            goto_action(xyz(page_two, Object::Null, real(300.0), integer(0))),
        ),
    );
    fixture.annot(
        0,
        with_action(link([10.0, 190.0, 60.0, 240.0]), goto_action(fit_r(page_two, [10.0, 20.0, 300.0, 400.0]))),
    );
    let input = dir.path("three.pdf");
    fixture.write(&input);

    let out = dir.path("three-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1, 2], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet_id = sheets(&doc)[0].1;
    let annots = annots(&doc, sheet_id);
    assert_eq!(annots.len(), 4);
    let matrices = paint_matrices(&page_content(&doc, sheet_id));
    let cell = placed_rect(&matrices[1], PAGE_W, PAGE_H);

    for (index, annot) in annots.iter().take(3).enumerate() {
        let dest = dest_of(annot);
        assert_eq!(dest.first().unwrap().as_reference().unwrap(), sheet_id);
        assert_eq!(dest[1].as_name().unwrap(), b"FitR", "view {index} reduces to the cell");
        let rect = [0, 1, 2, 3].map(|slot| number(&dest[slot + 2]).unwrap());
        assert!(close(rect, cell, TOLERANCE), "cell rect {rect:?} != {cell:?}");
    }
    // An exact rectangle is mapped and clipped, not replaced by the whole cell.
    let dest = dest_of(&annots[3]);
    let rect = [0, 1, 2, 3].map(|slot| number(&dest[slot + 2]).unwrap());
    let expected = expected_rect(&matrices[1], [10.0, 20.0, 300.0, 400.0]);
    assert!(close(rect, expected, TOLERANCE), "FitR {rect:?} != {expected:?}");
}

#[test]
fn link_to_an_unselected_page_is_dropped() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    let page_four = fixture.page(3);
    fixture.annot(
        0,
        with_action(link([10.0, 10.0, 60.0, 60.0]), goto_action(xyz(page_four, real(1.0), real(2.0), Object::Null))),
    );
    fixture.annot(0, with_action(link([10.0, 70.0, 60.0, 120.0]), uri_action("https://example.com/kept")));
    let input = dir.path("subset.pdf");
    fixture.write(&input);

    let out = dir.path("subset-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1, 2], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 1);
    let annots = annots(&doc, sheets[0].1);
    assert_eq!(annots.len(), 1, "the destination outside the selection is dropped");
    assert!(annots[0].get(b"A").is_ok(), "the URI link survives");
    assert!(annots[0].get(b"Dest").is_err());
}

#[test]
fn a_sheet_without_links_has_no_annots_key() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    fixture.annot(0, with_action(link([10.0, 10.0, 60.0, 60.0]), uri_action("https://example.com/one")));
    let input = dir.path("half.pdf");
    fixture.write(&input);

    let out = dir.path("half-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 2);
    assert!(has_annots(&doc, sheets[0].1), "the first sheet carries the link");
    assert!(!has_annots(&doc, sheets[1].1), "a sheet without links gets no /Annots key at all");
}

#[test]
fn booklet_link_targets_correct_sheet_and_slot() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    let page_three = fixture.page(2);
    fixture.annot(
        0,
        with_action(
            link([10.0, 10.0, 60.0, 60.0]),
            goto_action(xyz(page_three, real(10.0), real(700.0), Object::Null)),
        ),
    );
    let input = dir.path("booklet.pdf");
    fixture.write(&input);

    let out = dir.path("booklet-out.pdf");
    impose(&input, &out, &NupOptions { booklet: true, ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 2, "four booklet pages make two sheets");
    // Saddle-stitch order puts page 3 in the second slot of the second sheet.
    let first = sheets[0].1;
    let second = sheets[1].1;
    let annots = annots(&doc, first);
    assert_eq!(annots.len(), 1, "the link rides on the sheet that carries page 1");
    let dest = dest_of(&annots[0]);
    assert_eq!(dest.first().unwrap().as_reference().unwrap(), second);
    let matrices = paint_matrices(&page_content(&doc, second));
    let expected = apply(&matrices[1], 10.0, 700.0);
    assert!((number(&dest[2]).unwrap() - expected.0).abs() <= TOLERANCE);
    assert!((number(&dest[3]).unwrap() - expected.1).abs() <= TOLERANCE);
}

#[test]
fn three_page_booklet_pads_without_links() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    fixture.annot(0, with_action(link([10.0, 10.0, 60.0, 60.0]), uri_action("https://example.com/three")));
    let input = dir.path("pad.pdf");
    fixture.write(&input);

    let out = dir.path("pad-out.pdf");
    impose(&input, &out, &NupOptions { booklet: true, ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 2, "three pages still fill two sheets");
    let carried: usize = sheets.iter().map(|(_, id)| annots(&doc, *id).len()).sum();
    assert_eq!(carried, 1, "the padding blank carries nothing and loses nothing");
}

#[test]
fn named_destinations_are_resolved_to_explicit_arrays() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    let page_three = fixture.page(2);
    fixture.named_destinations(
        &[
            ("sec1", xyz(page_three, real(10.0), real(700.0), Object::Null)),
            ("sec2", xyz(page_three, real(20.0), real(600.0), Object::Null)),
        ],
        Some(("old", xyz(page_three, real(30.0), real(500.0), Object::Null))),
    );
    fixture.annot(0, with_dest(link([10.0, 10.0, 60.0, 60.0]), name("sec1")));
    fixture.annot(0, with_dest(link([10.0, 70.0, 60.0, 120.0]), Object::Name(b"old".to_vec())));
    let input = dir.path("named.pdf");
    fixture.write(&input);

    let out = dir.path("named-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet_id = sheets(&doc)[0].1;
    let annots = annots(&doc, sheet_id);
    assert_eq!(annots.len(), 2, "a name tree and the legacy /Dests dictionary both resolve");
    for annot in &annots {
        let dest = dest_of(annot);
        assert!(matches!(dest.first(), Some(Object::Reference(_))), "an explicit array names a sheet: {dest:?}");
        assert_ne!(dest.first().unwrap().as_reference().unwrap(), sheet_id, "page 3 is on the second sheet");
    }
    assert!(annots.iter().all(|annot| annot.get(b"Dest").is_ok() && annot.get(b"A").is_err()));
}

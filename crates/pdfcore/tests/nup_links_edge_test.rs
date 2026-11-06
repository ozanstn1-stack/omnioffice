//! N-up link carrying under hostile input: only links travel, and malformed
//! annotations are dropped instead of failing the imposition.

#[path = "common/link_fixtures.rs"]
mod fixtures;

use fixtures::*;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use pdfcore::nup::NupOptions;

const PAGE_W: f64 = 600.0;
const PAGE_H: f64 = 800.0;
/// The sheet content stream stores matrices with four decimals, so a
/// rectangle rebuilt from it lands a few hundredths away from the exact one.
const TOLERANCE: f64 = 0.05;

fn expected_rect(matrix: &[f64; 6], rect: [f64; 4]) -> [f64; 4] {
    let mapped = map_rect(matrix, rect);
    let cell = placed_rect(matrix, PAGE_W, PAGE_H);
    [mapped[0].max(cell[0]), mapped[1].max(cell[1]), mapped[2].min(cell[2]), mapped[3].min(cell[3])]
}

#[test]
fn only_links_survive_and_unsupported_actions_are_dropped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    fixture.annot(0, {
        let mut annot = link([10.0, 10.0, 60.0, 60.0]);
        annot.set("Subtype", Object::Name(b"Text".to_vec()));
        annot
    });
    fixture.annot(0, {
        let mut annot = link([10.0, 70.0, 60.0, 120.0]);
        annot.set("Subtype", Object::Name(b"Highlight".to_vec()));
        annot
    });
    fixture.annot(0, {
        let mut annot = link([10.0, 130.0, 60.0, 180.0]);
        annot.set("Subtype", Object::Name(b"Widget".to_vec()));
        annot
    });
    // Link subtypes whose action cannot mean the same thing on a sheet.
    let unsupported =
        [dictionary_launch(), dictionary_gotor(), dictionary_javascript(), dictionary_named(), dictionary_no_target()];
    for (index, action) in unsupported.into_iter().enumerate() {
        let y = 190.0 + index as f64 * 60.0;
        fixture.annot(0, with_action(link([10.0, y, 60.0, y + 50.0]), action));
    }
    let input = dir.path("subtypes.pdf");
    fixture.write(&input);

    let out = dir.path("subtypes-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("the imposition still produces a readable PDF");
    let sheets = sheets(&doc);
    assert_eq!(sheets.len(), 1);
    assert!(!has_annots(&doc, sheets[0].1), "no annotation of those kinds survives");
}

fn dictionary_launch() -> Dictionary {
    dictionary! { "S" => "Launch", "F" => Object::string_literal("other.pdf") }
}

fn dictionary_gotor() -> Dictionary {
    dictionary! { "S" => "GoToR", "F" => Object::string_literal("other.pdf") }
}

fn dictionary_javascript() -> Dictionary {
    dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("app.alert(1)") }
}

fn dictionary_named() -> Dictionary {
    dictionary! { "S" => "Named", "N" => "NextPage" }
}

fn dictionary_no_target() -> Dictionary {
    dictionary! { "T" => Object::string_literal("nothing") }
}

#[test]
fn broken_annotations_are_tolerated() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    let dangling: ObjectId = (9999, 0);
    let orphan_action = fixture.add_object(Object::Dictionary(dictionary! {
        "S" => "URI",
        "URI" => Object::string_literal("https://example.com/loop"),
    }));
    // An action that lists itself as its own `/Next`: following it would hang.
    fixture.patch(orphan_action, "Next", Object::Reference(orphan_action));
    let mut cyclic_tree = Dictionary::new();
    let cyclic_id = fixture.add_object(Object::Dictionary(cyclic_tree.clone()));
    cyclic_tree.set("Kids", Object::Array(vec![Object::Reference(cyclic_id)]));
    fixture.patch(cyclic_id, "Kids", Object::Array(vec![Object::Reference(cyclic_id)]));
    let catalog_id = fixture.catalog_id();
    let tree_id = fixture.add_object(Object::Dictionary(cyclic_tree));
    let names_id = fixture.add_object(Object::Dictionary(dictionary! { "Dests" => Object::Reference(tree_id) }));
    fixture.patch(catalog_id, "Names", Object::Reference(names_id));

    let mut broken: Vec<Object> = vec![
        Object::Integer(5),
        Object::Reference(dangling),
        // A link without a rectangle.
        Object::Dictionary(dictionary! { "Type" => "Annot", "Subtype" => "Link" }),
    ];
    let mut dangling_action = link([10.0, 10.0, 60.0, 60.0]);
    dangling_action.set("A", Object::Reference(dangling));
    broken.push(Object::Reference(fixture.add_object(Object::Dictionary(dangling_action))));
    broken.push(Object::Reference(fixture.add_object(Object::Dictionary(with_action(
        link([10.0, 70.0, 60.0, 120.0]),
        dictionary! { "S" => "GoTo", "D" => Object::Reference(dangling) },
    )))));
    // A link whose action points at a `/Next` self-loop.
    let mut looped = link([10.0, 130.0, 60.0, 180.0]);
    looped.set("A", Object::Reference(orphan_action));
    broken.push(Object::Reference(fixture.add_object(Object::Dictionary(looped))));
    broken.push(Object::Reference(
        fixture.add_object(Object::Dictionary(with_dest(link([10.0, 130.0, 60.0, 180.0]), name("missing")))),
    ));
    let valid = fixture.annot(0, with_action(link([10.0, 190.0, 60.0, 240.0]), uri_action("https://example.com/kept")));
    broken.push(Object::Reference(valid));
    fixture.raw_annots(0, Object::Array(broken));
    // A page whose `/Annots` is not even an array.
    fixture.raw_annots(1, Object::Integer(7));
    let input = dir.path("broken.pdf");
    fixture.write(&input);

    let out = dir.path("broken-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("broken input still imposes");
    let carried = annots(&doc, sheets(&doc)[0].1);
    assert_eq!(carried.len(), 2, "the two readable links survive, the broken ones do not");
    let kept: Vec<Vec<u8>> = carried
        .iter()
        .filter_map(|annot| annot.get(b"A").ok()?.as_dict().ok()?.get(b"URI").ok()?.as_str().ok().map(<[u8]>::to_vec))
        .collect();
    assert!(kept.iter().any(|uri| uri == b"https://example.com/kept"), "the valid link is kept: {kept:?}");
    assert!(
        carried.iter().all(|annot| annot.get(b"A").unwrap().as_dict().unwrap().get(b"Next").is_err()),
        "the action chain is not copied, so a self-loop cannot travel"
    );
}

#[test]
fn annotations_are_copied_not_shared() {
    let dir = TestDir::new();
    let specs = [PageSpec::default(), PageSpec::default(), PageSpec::default(), PageSpec::default()];
    let mut fixture = Fixture::new(&specs);
    for index in 0..4 {
        let mut annot =
            with_action(link([10.0, 10.0, 60.0, 60.0]), uri_action(&format!("https://example.com/{index}")));
        annot.set("StructParent", Object::Integer(3));
        fixture.annot(index, annot);
    }
    let input = dir.path("shared.pdf");
    fixture.write(&input);

    let out = dir.path("shared-out.pdf");
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let mut seen: Vec<ObjectId> = Vec::new();
    for (_, sheet_id) in sheets(&doc) {
        let page = doc.get_dictionary(sheet_id).expect("sheet");
        let items = page.get(b"Annots").expect("annots").as_array().expect("array").clone();
        for item in items {
            let id = item.as_reference().expect("indirect annotation");
            assert!(!seen.contains(&id), "an annotation object is shared between sheets: {id:?}");
            seen.push(id);
            let annot = doc.get_dictionary(id).expect("annotation");
            assert_eq!(annot.get(b"P").unwrap().as_reference().unwrap(), sheet_id, "owning sheet");
            assert!(annot.get(b"StructParent").is_err(), "the structure link is not carried over");
            assert!(annot.get(b"AP").is_err());
        }
    }
    assert_eq!(seen.len(), 4, "every page's link is carried once");
}

#[test]
fn a_link_partly_outside_the_page_is_clipped() {
    let dir = TestDir::new();
    let mut fixture = Fixture::new(&[PageSpec::default(), PageSpec::default()]);
    fixture.annot(0, with_action(link([550.0, 750.0, 700.0, 900.0]), uri_action("https://example.com/edge")));
    fixture.annot(0, with_action(link([700.0, 900.0, 800.0, 950.0]), uri_action("https://example.com/away")));
    let input = dir.path("edge.pdf");
    fixture.write(&input);

    let out = dir.path("edge-out.pdf");
    impose(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });
    let doc = Document::load(&out).expect("load output");
    let sheet_id = sheets(&doc)[0].1;
    let annots = annots(&doc, sheet_id);
    assert_eq!(annots.len(), 1, "a link entirely outside the artwork is dropped");
    let matrix = paint_matrices(&page_content(&doc, sheet_id))[0];
    let expected = expected_rect(&matrix, [550.0, 750.0, 700.0, 900.0]);
    let rect = rect_of(annots[0].get(b"Rect").expect("rect")).expect("rect array");
    assert!(close(rect, expected, TOLERANCE), "clipped rect {rect:?} != {expected:?}");
    let cell = placed_rect(&matrix, PAGE_W, PAGE_H);
    assert!(rect[2] <= cell[2] + TOLERANCE && rect[3] <= cell[3] + TOLERANCE);
}

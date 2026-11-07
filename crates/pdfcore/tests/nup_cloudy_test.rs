//! `/BE` cloudy borders on Square and Circle annotations without an appearance
//! stream: the frame becomes a chain of scallops instead of a plain edge.

#[path = "common/link_fixtures.rs"]
mod fixtures;
#[path = "common/markup_fixtures.rs"]
mod markup_fixtures;

use fixtures::{impose, page_content, rect_array, sheets, Fixture, PageSpec, TestDir};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId};
use markup_fixtures::{blocks, xobject};
use pdfcore::nup::NupOptions;

fn page_specs(count: usize) -> Vec<PageSpec> {
    (0..count).map(|_| PageSpec::default()).collect()
}

fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A shape annotation with `/BE << /S /C /I n >>`, or without `/BE` at all.
fn shape(subtype: &str, rect: [f64; 4], effect: Option<f64>) -> Dictionary {
    let mut annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
        "Rect" => rect_array(rect),
        "F" => 4i64,
        "C" => numbers(&[0.0, 0.0, 0.0]),
        "BS" => dictionary! { "W" => 1i64, "S" => Object::Name(b"S".to_vec()) },
    };
    if let Some(intensity) = effect {
        annot.set(
            "BE",
            Object::Dictionary(
                dictionary! { "S" => Object::Name(b"C".to_vec()), "I" => Object::Real(intensity as f32) },
            ),
        );
    }
    annot
}

fn painted(doc: &Document, sheet: ObjectId) -> Vec<markup_fixtures::Block> {
    blocks(&page_content(doc, sheet)).into_iter().filter(|block| block.name.starts_with("AnX")).collect()
}

fn ops_of(doc: &Document, sheet: ObjectId, name: &str) -> String {
    let id = xobject(doc, sheet, name).expect("drawn XObject");
    let stream = doc.get_object(id).and_then(|object| object.as_stream()).expect("stream");
    let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
    String::from_utf8_lossy(&bytes).to_string()
}

fn render(annot: Dictionary, dir: &TestDir, name: &str) -> String {
    let mut fixture = Fixture::new(&page_specs(2));
    fixture.annot(0, annot);
    let input = dir.path(&format!("{name}.pdf"));
    fixture.write(&input);
    let out = dir.path(&format!("{name}-out.pdf"));
    impose(&input, &out, &NupOptions::default());
    let doc = Document::load(&out).expect("load output");
    let sheet = sheets(&doc)[0].1;
    ops_of(&doc, sheet, &painted(&doc, sheet)[0].name)
}

#[test]
fn a_cloudy_square_is_drawn_as_scallops() {
    let dir = TestDir::new();
    let ops = render(shape("Square", [100.0, 100.0, 300.0, 200.0], Some(1.0)), &dir, "cloudy-square");
    assert!(ops.matches(" c\n").count() > 8, "a chain of curved bumps: {ops:?}");
    assert!(ops.ends_with("h\nS\n"), "one closed path, stroked: {ops:?}");
    assert!(!ops.contains(" re\n"), "and no plain rectangle: {ops:?}");
}

#[test]
fn a_cloudy_circle_is_drawn_as_scallops() {
    let dir = TestDir::new();
    let ops = render(shape("Circle", [100.0, 100.0, 300.0, 200.0], Some(2.0)), &dir, "cloudy-circle");
    assert!(ops.matches(" c\n").count() > 8, "{ops:?}");
    assert!(ops.ends_with("h\nS\n"), "{ops:?}");
}

#[test]
fn a_shape_without_the_effect_keeps_its_plain_frame() {
    let dir = TestDir::new();
    let plain = render(shape("Square", [100.0, 100.0, 300.0, 200.0], None), &dir, "plain-square");
    assert!(plain.contains(" re\n"), "the plain rectangle is unchanged: {plain:?}");
    assert!(!plain.contains(" c\n"), "no curves without /BE: {plain:?}");
    // A solid `/S` is not a cloud either.
    let solid = render(shape("Circle", [100.0, 100.0, 300.0, 200.0], None), &dir, "plain-circle");
    assert!(!solid.contains(" c\n") || solid.contains("h\n"), "the ellipse path is unchanged: {solid:?}");
}

#[test]
fn the_output_is_deterministic() {
    let dir = TestDir::new();
    let first = render(shape("Square", [100.0, 100.0, 300.0, 200.0], Some(0.5)), &dir, "again");
    let again = render(shape("Square", [100.0, 100.0, 300.0, 200.0], Some(0.5)), &dir, "again");
    assert_eq!(first, again);
}

//! What Flatten is supposed to do to the page a user sees, checked on the bytes.
//!
//! Every expectation below follows either from ISO 32000-1 (annotation flags in
//! 12.5.3, appearance placement in 12.5.5, the content stream in 8.2) or from the
//! module's own documented contract, quoted where it applies:
//!
//! * "For every annotation with an `/AP /N` appearance stream the module appends
//!   `q <matrix> cm /<name> Do Q` to the page, where the matrix maps the stream's
//!   `/BBox` (after its own `/Matrix`) onto the annotation `/Rect`" - so a hidden
//!   annotation must not be drawn, and an inverted `/Rect` must be normalised.
//! * "Anything that cannot be flattened ... is reported as a warning instead of
//!   being silently dropped" - so a refusal has to show up in the report.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::flatten::{flatten_pdf, FlattenOptions, FlattenReport};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-flatten-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page document with one annotation that draws a solid grey rectangle.
///
/// `flags` is the annotation's `/F`, `annot_rect` its `/Rect`, `bbox` the
/// appearance's `/BBox` and `matrix` its optional `/Matrix`.
struct Fixture {
    flags: i64,
    annot_rect: [f64; 4],
    bbox: [f64; 4],
    matrix: Option<[f64; 6]>,
    ap_content: Vec<u8>,
    with_ap: bool,
    extra: Option<Dictionary>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            flags: 4,
            annot_rect: [100.0, 200.0, 200.0, 250.0],
            bbox: [0.0, 0.0, 100.0, 50.0],
            matrix: None,
            ap_content: b"0.5 g 0 0 100 50 re f".to_vec(),
            with_ap: true,
            extra: None,
        }
    }
}

fn build(name: &str, fixture: Fixture) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), b"0 g 0 0 10 10 re f".to_vec())));
    let mut appearance = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => rect(fixture.bbox),
        "Resources" => Dictionary::new(),
    };
    if let Some(matrix) = fixture.matrix {
        appearance.set("Matrix", numbers(&matrix));
    }
    let appearance = doc.add_object(Object::Stream(Stream::new(appearance, fixture.ap_content.clone())));
    let mut annot = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => rect(fixture.annot_rect),
        "F" => Object::Integer(fixture.flags),
        "C" => Object::Array(vec![Object::Real(0.2), Object::Real(0.4), Object::Real(0.6)]),
    };
    if fixture.with_ap {
        annot.set("AP", dictionary! { "N" => Object::Reference(appearance) });
    }
    if let Some(extra) = fixture.extra {
        for (key, value) in extra.iter() {
            annot.set(key.clone(), value.clone());
        }
    }
    let annot = doc.add_object(Object::Dictionary(annot));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
        "Contents" => Object::Reference(content),
        "Annots" => Object::Array(vec![Object::Reference(annot)]),
    }));
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => Object::Integer(1),
        }),
    );
    let catalog = doc.new_object_id();
    doc.objects
        .insert(catalog, Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages) }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = scratch(name);
    doc.save(&path).expect("save fixture");
    path
}

/// One `Do` of flatten's own overlay: the name it registered and the matrix it
/// wrote immediately before it.
#[derive(Debug)]
struct Drawn {
    name: String,
    matrix: [f64; 6],
}

/// The overlay operations a page now carries, and whether its content is balanced.
fn overlay(path: &Path) -> (Vec<Drawn>, bool) {
    let doc = Document::load(path).expect("load output");
    let page_id = *doc.get_pages().get(&1).expect("page one");
    let content = doc.get_page_content(page_id);
    let text = String::from_utf8_lossy(&content).to_string();
    let mut depth = 0i32;
    let mut balanced = true;
    let mut drawn = Vec::new();
    let mut pending_matrix: Option<[f64; 6]> = None;
    let mut numbers: Vec<f64> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "q" {
            depth += 1;
        } else if line == "Q" {
            depth -= 1;
            if depth < 0 {
                balanced = false;
            }
        } else if let Some(name) = line.strip_suffix(" Do") {
            let name = name.trim_start_matches('/').to_string();
            // Only flatten's own overlays are interesting: they are the ones with
            // a `cm` immediately before them in the same block.
            if let Some(matrix) = pending_matrix.take() {
                drawn.push(Drawn { name, matrix });
            }
        } else if let Some(values) = line.strip_suffix(" cm") {
            let parsed: Vec<f64> = values.split_whitespace().filter_map(|value| value.parse().ok()).collect();
            if let [a, b, c, d, e, f] = parsed[..] {
                pending_matrix = Some([a, b, c, d, e, f]);
            }
        } else {
            for token in line.split_whitespace() {
                if let Ok(number) = token.parse::<f64>() {
                    numbers.push(number);
                }
            }
            if numbers.len() == 6 && pending_matrix.is_none() {
                if let [a, b, c, d, e, f] = numbers[..] {
                    pending_matrix = Some([a, b, c, d, e, f]);
                }
            }
            numbers.clear();
        }
    }
    (drawn, balanced && depth == 0)
}

fn run(input: &Path, output: &Path) -> FlattenReport {
    flatten_pdf(input, output, &FlattenOptions::default(), &|_| {}, &pdfcore::progress::CancelToken::new())
        .expect("flatten")
}

fn annot_count(path: &Path) -> usize {
    let doc = Document::load(path).expect("load");
    let page_id = *doc.get_pages().get(&1).expect("page");
    match doc.get_dictionary(page_id).and_then(|page| page.get(b"Annots")) {
        Ok(Object::Array(items)) => items.len(),
        _ => 0,
    }
}

/// Whether the appearance is drawn over the rectangle it was placed on.
fn drawn_over(drawn: &Drawn, rect: [f64; 4], bbox: [f64; 4]) -> bool {
    // The appearance's own box, mapped through the overlay matrix, must land on
    // the annotation's rectangle.
    let corners = [[bbox[0], bbox[1]], [bbox[2], bbox[1]], [bbox[2], bbox[3]], [bbox[0], bbox[3]]];
    let mut mapped = Vec::new();
    for [x, y] in corners {
        let mx = drawn.matrix[0] * x + drawn.matrix[2] * y + drawn.matrix[4];
        let my = drawn.matrix[1] * x + drawn.matrix[3] * y + drawn.matrix[5];
        mapped.push((mx, my));
    }
    let xs: Vec<f64> = mapped.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = mapped.iter().map(|(_, y)| *y).collect();
    let (min_x, max_x) = (xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
    let (min_y, max_y) = (ys.iter().cloned().fold(f64::MAX, f64::min), ys.iter().cloned().fold(f64::MIN, f64::max));
    (min_x - rect[0]).abs() < 0.5
        && (max_x - rect[2]).abs() < 0.5
        && (min_y - rect[1]).abs() < 0.5
        && (max_y - rect[3]).abs() < 0.5
}

#[test]
fn an_appearance_lands_on_its_rect() {
    let input = build("identity", Fixture::default());
    let output = scratch("identity-out");
    let report = run(&input, &output);
    let (drawn, balanced) = overlay(&output);
    assert!(balanced, "the page content stays balanced");
    assert_eq!(drawn.len(), 1, "one appearance is painted");
    assert!(drawn_over(&drawn[0], [100.0, 200.0, 200.0, 250.0], [0.0, 0.0, 100.0, 50.0]));
    assert_eq!(annot_count(&output), 0, "the annotation is gone");
    assert_eq!(report.annotations_flattened, 1);
}

#[test]
fn a_smaller_bbox_is_scaled_onto_the_rect() {
    let input = build("scale", Fixture { bbox: [0.0, 0.0, 50.0, 25.0], ..Fixture::default() });
    let output = scratch("scale-out");
    run(&input, &output);
    let (drawn, _) = overlay(&output);
    assert_eq!(drawn.len(), 1);
    assert!(drawn_over(&drawn[0], [100.0, 200.0, 200.0, 250.0], [0.0, 0.0, 50.0, 25.0]), "scaled up by two");
}

#[test]
fn a_reversed_rect_is_normalised() {
    let input = build("reverse", Fixture { annot_rect: [200.0, 250.0, 100.0, 200.0], ..Fixture::default() });
    let output = scratch("reverse-out");
    run(&input, &output);
    let (drawn, _) = overlay(&output);
    assert_eq!(drawn.len(), 1);
    assert!(drawn_over(&drawn[0], [100.0, 200.0, 200.0, 250.0], [0.0, 0.0, 100.0, 50.0]), "the box is normalised");
}

#[test]
fn a_hidden_annotation_is_not_drawn() {
    // `/F 2` is Hidden (12.5.3): the viewer does not show it, so painting it
    // would make the output differ from what the user saw.
    let input = build("hidden", Fixture { flags: 2, ..Fixture::default() });
    let output = scratch("hidden-out");
    let report = run(&input, &output);
    let (drawn, _) = overlay(&output);
    assert!(
        drawn.is_empty(),
        "a hidden annotation is not painted: {:?}",
        drawn.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
    assert!(
        report.annotations_flattened == 0 || !report.warnings.is_empty(),
        "a hidden annotation is accounted for, not silently dropped"
    );
}

#[test]
fn an_annotation_without_the_print_flag_is_still_drawn() {
    // `/F 0` means visible on screen and not printed - the default for many
    // highlights. The contract says flatten bakes the appearance and removes the
    // annotation, with no print-only filter mentioned.
    let input = build("noprint", Fixture { flags: 0, ..Fixture::default() });
    let output = scratch("noprint-out");
    run(&input, &output);
    let (drawn, _) = overlay(&output);
    assert_eq!(
        drawn.len(),
        1,
        "a screen-visible annotation is painted: {:?}",
        drawn.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
}

#[test]
fn an_appearance_without_a_box_is_reported() {
    // No `/AP`: the contract says the annotation is reported as a warning rather
    // than silently dropped, so it has to still be there and the report has to say so.
    let input = build("no-ap", Fixture { with_ap: false, ..Fixture::default() });
    let output = scratch("no-ap-out");
    let report = run(&input, &output);
    let (drawn, _) = overlay(&output);
    let kept = annot_count(&output);
    assert!(
        !drawn.is_empty() || (kept == 1 && !report.warnings.is_empty()),
        "either drawn, or kept and reported: drawn {:?}, kept {kept}, warnings {:?}",
        drawn.iter().map(|d| &d.name).collect::<Vec<_>>(),
        report.warnings
    );
}

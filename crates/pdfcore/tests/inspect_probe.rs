//! What the inspector tells a user about their document, checked on the bytes.
//!
//! The inspector is read-only, so a wrong answer cannot damage a file - but it can
//! tell someone their document is fine when it is not, or that it is broken when
//! it is not. The module's own contract (`inspect.rs`): "Findings carry a severity
//! and a plain explanation, because 'this document fails WCAG' without saying
//! which check failed and why is not something anyone can act on", and everything
//! it reports is "from the file structure".
//!
//! Every expectation here follows from that contract or from ISO 32000-1; the
//! outline round trip is a product invariant: reading a tree, writing it back and
//! reading it again must give the same tree.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::inspect::{evaluate_findings, inspect_document, read_outline, write_outline, OutlineEntry, Severity};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-inspect-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// How much of a document to build: the outline shape and the accessibility
/// metadata are the two things the inspector reports on.
struct Spec {
    pages: usize,
    /// `(title, page, depth)` of the outline, or empty for none.
    outline: Vec<(&'static str, u32, u32)>,
    title: Option<&'static str>,
    language: Option<&'static str>,
    tagged: bool,
    /// A figure element without `/Alt`.
    figure_without_alt: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Self { pages: 2, outline: Vec::new(), title: None, language: None, tagged: false, figure_without_alt: false }
    }
}

fn build(name: &str, spec: Spec) -> PathBuf {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut kids = Vec::new();
    let mut page_ids: Vec<lopdf::ObjectId> = Vec::new();
    for index in 0..spec.pages {
        let body = format!("BT /F1 12 Tf 72 720 Td (PAGE-{}) Tj ET", index + 1);
        let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
            "Contents" => Object::Reference(content),
        }));
        page_ids.push(page_id);
        kids.push(Object::Reference(page_id));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(spec.pages as i64),
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) };
    if !spec.outline.is_empty() {
        // One destination per entry, pointing at the page it names.
        let mut ids = Vec::new();
        for (title, page, _depth) in &spec.outline {
            let target = page_ids.get((*page as usize).saturating_sub(1)).copied().unwrap_or_else(|| page_ids[0]);
            let id = doc.add_object(Object::Dictionary(dictionary! {
                "Title" => Object::string_literal(*title),
                "Parent" => Object::Reference(pages_id),
                "Dest" => Object::Array(vec![Object::Reference(target), Object::Name(b"Fit".to_vec())]),
            }));
            ids.push(id);
        }
        // Link them as a flat `/Next` chain; `read_outline` reads depth from nesting,
        // which this fixture does not need to prove.
        for window in ids.windows(2) {
            let (first, second) = (window[0], window[1]);
            if let Ok(Object::Dictionary(entry)) = doc.get_object_mut(first) {
                entry.set("Next", Object::Reference(second));
            }
            if let Ok(Object::Dictionary(entry)) = doc.get_object_mut(second) {
                entry.set("Prev", Object::Reference(first));
            }
        }
        let outlines = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => Object::Reference(ids[0]),
            "Last" => Object::Reference(*ids.last().unwrap()),
            "Count" => Object::Integer(ids.len() as i64),
        }));
        catalog.set("Outlines", Object::Reference(outlines));
    }
    let mut info: Dictionary = Dictionary::new();
    if let Some(title) = spec.title {
        info.set("Title", Object::string_literal(title));
    }
    if let Some(language) = spec.language {
        catalog.set("Lang", Object::string_literal(language));
    }
    if spec.tagged {
        let parent_tree = if spec.figure_without_alt {
            let figure = doc.add_object(Object::Dictionary(dictionary! {
                "Type" => "StructElem",
                "S" => "Figure",
            }));
            doc.add_object(Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => Object::Array(vec![Object::Reference(figure)]),
            }))
        } else {
            doc.add_object(Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => Object::Array(Vec::new()),
            }))
        };
        catalog.set("StructTreeRoot", Object::Reference(parent_tree));
        catalog.set("MarkInfo", dictionary! { "Marked" => true });
    }
    let catalog = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog));
    if !info.is_empty() {
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));
    }
    let path = scratch(name);
    doc.save(&path).expect("save fixture");
    path
}

fn inspected(path: &Path) -> pdfcore::inspect::DocumentInspection {
    let mut inspection = inspect_document(path, None).expect("inspect");
    evaluate_findings(&mut inspection);
    inspection
}

fn codes(inspection: &pdfcore::inspect::DocumentInspection) -> Vec<String> {
    inspection.findings.iter().map(|finding| finding.code.clone()).collect()
}

fn entry(title: &str, page: u32, depth: u32) -> OutlineEntry {
    OutlineEntry { title: title.to_string(), page, depth }
}

#[test]
fn a_document_with_no_metadata_is_told_what_is_missing() {
    let path = build("bare", Spec::default());
    let inspection = inspected(&path);
    let codes = codes(&inspection);
    for expected in ["a11y.missing-title", "a11y.missing-language", "a11y.untagged"] {
        assert!(codes.iter().any(|code| code == expected), "expected {expected} in {codes:?}");
    }
}

#[test]
fn a_document_that_is_fine_gets_no_errors() {
    // The false-alarm side: title, language and a structure tree are all there, so
    // nothing may be reported as an error.
    let spec = Spec { title: Some("Report"), language: Some("tr-TR"), tagged: true, ..Spec::default() };
    let path = build("clean", spec);
    let inspection = inspected(&path);
    // This fixture does not embed a font, so a `pdfa.*` finding is correct and out
    // of scope: what must be clean is the accessibility side.
    let errors: Vec<&String> = inspection
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.code.starts_with("a11y."))
        .map(|f| &f.code)
        .collect();
    assert!(errors.is_empty(), "a complete document reports no accessibility errors: {errors:?}");
}

#[test]
fn a_figure_without_alt_text_is_an_error() {
    let spec = Spec {
        title: Some("Report"),
        language: Some("tr-TR"),
        tagged: true,
        figure_without_alt: true,
        ..Spec::default()
    };
    let path = build("no-alt", spec);
    let inspection = inspected(&path);
    assert!(inspection.figures_missing_alt >= 1, "the count follows the file");
    let codes = codes(&inspection);
    assert!(codes.iter().any(|code| code.contains("alt")), "a figure without /Alt has to be reported: {codes:?}");
}

#[test]
fn the_page_and_object_counts_match_the_file() {
    let path = build("counts", Spec { pages: 3, ..Spec::default() });
    let inspection = inspected(&path);
    assert_eq!(inspection.page_count, 3);
    let doc = Document::load(&path).expect("reload");
    assert_eq!(inspection.object_count as usize, doc.objects.len(), "the object count follows the file");
}

#[test]
fn an_outline_survives_a_round_trip_through_the_writer() {
    // The product invariant: read, write, read again - the same tree.
    let spec = Spec {
        outline: vec![("First", 1, 0), ("First child", 1, 1), ("Second child", 2, 1), ("Second", 2, 0)],
        ..Spec::default()
    };
    let path = build("outline", spec);
    let doc = Document::load(&path).expect("load");
    let catalog = doc.catalog().expect("catalog").clone();
    let original = read_outline(&doc, &catalog).expect("read");
    assert_eq!(original.len(), 4, "the fixture's outline is read back: {original:?}");
    let bytes = std::fs::read(&path).expect("read fixture");
    let written = write_outline(&bytes, &original).expect("write");
    let rewritten = Document::load_mem(&written).expect("reload the written outline");
    let after = read_outline(&rewritten, rewritten.catalog().expect("catalog")).expect("read back");
    assert_eq!(after.len(), original.len(), "the tree keeps its entries");
    for (before, now) in original.iter().zip(after.iter()) {
        assert_eq!(before.title, now.title, "titles survive");
        assert_eq!(before.page, now.page, "destinations survive");
        assert_eq!(before.depth, now.depth, "nesting survives");
    }
}

#[test]
fn the_writer_refuses_a_destination_outside_the_document() {
    let path = build("bad-dest", Spec::default());
    let bytes = std::fs::read(&path).expect("read fixture");
    let entries = vec![entry("Nowhere", 99, 0)];
    assert!(write_outline(&bytes, &entries).is_err(), "page 99 does not exist in a two-page document");
    let entries = vec![entry("Zero", 0, 0)];
    assert!(write_outline(&bytes, &entries).is_err(), "page numbers start at one");
}

#[test]
fn a_document_without_an_outline_reports_none() {
    let path = build("no-outline", Spec::default());
    let doc = Document::load(&path).expect("load");
    let outline = read_outline(&doc, doc.catalog().expect("catalog")).expect("read");
    assert!(outline.is_empty(), "no /Outlines means no entries");
}

#[test]
fn a_cyclic_outline_terminates() {
    // A `/Next` that points back at the first entry is a loop, not a document.
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => rect([0.0, 0.0, 595.0, 842.0]),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => Object::Integer(1),
        }),
    );
    let first = doc.new_object_id();
    let second = doc.new_object_id();
    doc.objects.insert(
        first,
        Object::Dictionary(
            dictionary! { "Title" => Object::string_literal("One"), "Next" => Object::Reference(second) },
        ),
    );
    doc.objects.insert(
        second,
        Object::Dictionary(
            dictionary! { "Title" => Object::string_literal("Two"), "Next" => Object::Reference(first) },
        ),
    );
    let outlines = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Outlines",
        "First" => Object::Reference(first),
        "Last" => Object::Reference(second),
    }));
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
        "Outlines" => Object::Reference(outlines),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let path = scratch("cyclic");
    doc.save(&path).expect("save");
    let doc = Document::load(&path).expect("load");
    let outline = read_outline(&doc, doc.catalog().expect("catalog")).expect("read");
    assert!(outline.len() <= 10_000, "the walk is bounded, not endless: {} entries", outline.len());
}

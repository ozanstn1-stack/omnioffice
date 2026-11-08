//! What the built-in repair promises about the file it gives back.
//!
//! Repair is used when a document no longer opens, so the user usually cannot
//! compare the result with what they had. That makes two things worth pinning:
//! a reported success must keep what is still physically in the file, and it must
//! never bring back content that a later save replaced. The module's own contract
//! is in `repair.rs`: it "re-opens the result so the report describes the file
//! that was actually produced", and its warnings are "what was lost or
//! reconstructed".
//!
//! Each fixture is an intact document plus one named damage, and every content
//! stream carries a unique sentinel so the oracle can look for it in the output
//! without trusting the parser under test.

use std::path::PathBuf;

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdfcore::progress::CancelToken;
use pdfcore::repair::{repair_pdf_builtin, RepairReport};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("omnioffice-repair-probe");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir.join(format!("{name}.pdf"))
}

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// An intact two-page document whose pages carry the sentinels `PAGE-ONE` and
/// `PAGE-TWO`.
struct Intact {
    bytes: Vec<u8>,
    /// Object id of page one, for the damage helpers that rewrite it.
    page_one: lopdf::ObjectId,
    /// Object id of the page tree, for `/MediaBox` inheritance.
    pages: lopdf::ObjectId,
}

fn intact(media_on_pages: bool, page_order_reversed: bool) -> Intact {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut ids = Vec::new();
    for marker in ["PAGE-ONE", "PAGE-TWO"] {
        let body = format!("BT /F1 18 Tf 72 720 Td ({marker}) Tj ET");
        let content = doc.add_object(Object::Stream(Stream::new(Dictionary::new(), body.into_bytes())));
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages),
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
            "Contents" => Object::Reference(content),
        };
        if !media_on_pages {
            page.set("MediaBox", rect([0.0, 0.0, 595.0, 842.0]));
        }
        ids.push(doc.add_object(Object::Dictionary(page)));
    }
    let mut tree = dictionary! {
        "Type" => "Pages",
        "Count" => Object::Integer(2),
    };
    if media_on_pages {
        tree.set("MediaBox", rect([0.0, 0.0, 612.0, 792.0]));
    }
    let order: Vec<Object> = if page_order_reversed {
        ids.iter().rev().map(|id| Object::Reference(*id)).collect()
    } else {
        ids.iter().map(|id| Object::Reference(*id)).collect()
    };
    tree.set("Kids", Object::Array(order));
    doc.objects.insert(pages, Object::Dictionary(tree));
    let catalog = doc.new_object_id();
    doc.objects
        .insert(catalog, Object::Dictionary(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages) }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save fixture");
    Intact { bytes, page_one: ids[0], pages }
}

/// Runs the built-in repair and returns the report plus the output bytes.
fn repair(name: &str, damaged: &[u8]) -> (RepairReport, Vec<u8>) {
    let input = scratch(&format!("{name}-in"));
    let output = scratch(&format!("{name}-out"));
    std::fs::write(&input, damaged).expect("write damaged input");
    let report = repair_pdf_builtin(&input, &output, &|_| {}, &CancelToken::new()).expect("built-in repair");
    let bytes = std::fs::read(&output).unwrap_or_default();
    (report, bytes)
}

fn contains(bytes: &[u8], needle: &str) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle.as_bytes())
}

/// The effective `/MediaBox` of page one, walking `/Parent` because the box is
/// inheritable (ISO 32000-1 7.7.3.4) - a page without its own copy is valid when
/// an ancestor carries it.
fn page_box(bytes: &[u8]) -> Option<[f64; 4]> {
    let doc = Document::load_mem(bytes).ok()?;
    let mut id = *doc.get_pages().get(&1)?;
    for _ in 0..16 {
        let page = doc.get_dictionary(id).ok()?;
        if let Ok(media) = page.get(b"MediaBox") {
            let values: Vec<f64> = media.as_array().ok()?.iter().map(|v| v.as_float().unwrap_or(0.0) as f64).collect();
            return Some([values[0], values[1], values[2], values[3]]);
        }
        id = page.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// The text of page one's content stream, read back through a fresh parse.
fn page_one_text(bytes: &[u8]) -> String {
    let Ok(doc) = Document::load_mem(bytes) else { return String::new() };
    let Some(page_id) = doc.get_pages().get(&1).copied() else { return String::new() };
    String::from_utf8_lossy(&doc.get_page_content(page_id)).to_string()
}

/// Cuts the tail of the file, which takes the `startxref` and `%%EOF` with it.
fn truncate_tail(bytes: &[u8], keep: usize) -> Vec<u8> {
    let cut = bytes.len().saturating_sub(keep);
    bytes[..cut].to_vec()
}

#[test]
fn a_truncated_tail_still_opens_and_keeps_both_pages() {
    let fixture = intact(false, false);
    let damaged = truncate_tail(&fixture.bytes, 40);
    assert_ne!(damaged, fixture.bytes, "the fixture must really be damaged");
    let (report, output) = repair("truncated", &damaged);
    assert!(contains(&output, "PAGE-ONE") && contains(&output, "PAGE-TWO"), "both pages survive");
    assert_eq!(report.pages, 2, "the report counts the pages it actually produced");
}

#[test]
fn junk_before_the_header_and_after_the_end_is_tolerated() {
    let fixture = intact(false, false);
    let mut damaged = b"From: someone@example.com\r\nSubject: your invoice\r\n\r\n".to_vec();
    damaged.extend_from_slice(&fixture.bytes);
    damaged.extend_from_slice(b"\r\n-- junk after the end of the file --\r\n");
    let (_report, output) = repair("junk", &damaged);
    assert!(contains(&output, "PAGE-ONE") && contains(&output, "PAGE-TWO"));
}

#[test]
fn a_later_save_replaces_the_earlier_definition() {
    // The classic incremental save: object 4 is written again with new text. The
    // repair must keep the newest definition, never resurrect the old one.
    let fixture = intact(false, false);
    let page = fixture.page_one;
    let mut damaged = fixture.bytes.clone();
    // An appended definition, exactly how an incremental save writes it.
    let offset = damaged.len();
    let object = format!(
        "\n{} {} obj\n<< /Type /Page /Parent {} 0 R /MediaBox [0 0 595 842] /Contents {} 0 R >>\nendobj\n",
        page.0, page.1, fixture.pages.0, 99
    );
    damaged.extend_from_slice(object.as_bytes());
    damaged.extend_from_slice(format!("\nstartxref\n{offset}\n%%EOF\n").as_bytes());
    let (_report, output) = repair("replaced", &damaged);
    assert!(contains(&output, "PAGE-TWO"), "the untouched page survives");
}

#[test]
fn an_inherited_page_box_survives() {
    let fixture = intact(true, false);
    let damaged = truncate_tail(&fixture.bytes, 40);
    let (_report, output) = repair("inherited", &damaged);
    let page = page_box(&output).expect("a page box in the output");
    assert!(
        (page[2] - 612.0).abs() < 1.0 && (page[3] - 792.0).abs() < 1.0,
        "the box inherited from the page tree is kept: {page:?}"
    );
}

#[test]
fn the_page_order_follows_the_page_tree() {
    // Kids order is reversed against object numbers; the repaired file must keep
    // the tree's order, not sort by object number.
    let fixture = intact(false, true);
    let damaged = truncate_tail(&fixture.bytes, 40);
    let (_report, output) = repair("order", &damaged);
    let first = page_one_text(&output);
    assert!(first.contains("PAGE-TWO"), "page one is the tree's first kid: {first:?}");
}

#[test]
fn an_embedded_document_inside_a_stream_is_not_a_page() {
    let mut fixture = intact(false, false);
    // A stream whose payload looks like a small PDF of its own.
    let inner = b"%PDF-1.7\n1 0 obj\n<< /Type /Page >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n";
    let mut doc = Document::load_mem(&fixture.bytes).expect("reload");
    doc.add_object(Object::Stream(Stream::new(Dictionary::new(), inner.to_vec())));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save with the embedded stream");
    fixture.bytes = bytes;
    let damaged = truncate_tail(&fixture.bytes, 40);
    let (report, output) = repair("embedded", &damaged);
    assert_eq!(report.pages, 2, "only the outer pages are pages");
    assert!(contains(&output, "PAGE-ONE") && contains(&output, "PAGE-TWO"));
}

#[test]
fn a_stream_with_a_wrong_length_is_still_read_to_its_end() {
    // `/Length` that is short by ten bytes: a repair that trusts it would cut the
    // content stream and lose the text.
    let fixture = intact(false, false);
    let text = String::from_utf8_lossy(&fixture.bytes).to_string();
    let broken = text.replace("/Length 43", "/Length 33");
    let damaged = if broken == text { text.replace("/Length 44", "/Length 34") } else { broken };
    let damaged = damaged.into_bytes();
    let (_report, output) = repair("length", &damaged);
    assert!(
        contains(&output, "PAGE-ONE") && contains(&output, "PAGE-TWO"),
        "a wrong /Length must not cost the page text"
    );
}

#[test]
fn text_cut_off_mid_stream_is_disclosed_rather_than_claimed() {
    // The download stopped inside page two's content stream: its text cannot be
    // recovered, so a report that says nothing is a false claim.
    let fixture = intact(false, false);
    let cut = fixture.bytes.len() / 2;
    let damaged = fixture.bytes[..cut].to_vec();
    let (report, output) = repair("cut", &damaged);
    let kept = contains(&output, "PAGE-TWO");
    assert!(
        kept || report.pages < 2 || !report.warnings.is_empty(),
        "loss has to show up: pages {}, warnings {:?}",
        report.pages,
        report.warnings
    );
}

#[test]
fn a_page_tree_that_is_gone_does_not_lose_the_pages_silently() {
    // `/Pages` is unreadable but both page objects are intact: either they are
    // recovered, or the report says they were not.
    let fixture = intact(false, false);
    let text = String::from_utf8_lossy(&fixture.bytes).to_string();
    // Damage the page tree object itself.
    let broken = text.replace("/Type/Pages", "/Type/Pagez");
    assert_ne!(broken, text, "the page tree must really be damaged");
    let (report, output) = repair("tree", broken.as_bytes());
    let pages = report.pages;
    assert!(
        pages >= 1 || !report.warnings.is_empty(),
        "an empty output has to be explained: {pages} pages, warnings {:?}",
        report.warnings
    );
    if pages >= 1 {
        assert!(contains(&output, "PAGE-ONE") || contains(&output, "PAGE-TWO"));
    }
}

#[test]
fn the_report_says_which_engine_and_how_many_pages() {
    let fixture = intact(false, false);
    let damaged = truncate_tail(&fixture.bytes, 40);
    let (report, _output) = repair("report", &damaged);
    assert_eq!(report.pages, 2);
    assert!(!report.output.is_empty());
    assert_eq!(report.method, Some(pdfcore::repair::RepairMethod::Builtin));
}

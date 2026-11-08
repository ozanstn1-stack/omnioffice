//! Where the exhibit label lands on an evidence bundle page.
//!
//! A bundle stamps every exhibit page with its label, and that label is drawn
//! into the page's content stream. The stamp is placed in display space - the
//! region a viewer shows, the crop box clipped to the media box (ISO 32000-1
//! 14.11.2), whose corner starts the page's own coordinates (7.7.3.3).
//!
//! The cover and index pages are generated, so a bundle always has them; the
//! exhibit pages come from the user's own files and can be cropped, rotated or
//! shifted. These cases pin the label against the page box, using differences
//! between two bundles that differ in one property only, so no expected number
//! depends on how wide the label happens to be.

use lopdf::{dictionary, Document, Object, Stream};
use pdfcore::evidence::{build_evidence_bundle, EvidenceDetails, EvidenceFile};

fn rect(values: [f64; 4]) -> Object {
    Object::Array(values.iter().map(|value| Object::Real(*value as f32)).collect())
}

/// A one-page exhibit with the given box, optionally cropped and rotated.
fn exhibit(media: [f64; 4], crop: Option<[f64; 4]>, rotate: i64) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content = doc.add_object(Stream::new(dictionary! {}, b"BT /F1 12 Tf 72 720 Td (exhibit) Tj ET".to_vec()));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => rect(media),
    };
    if let Some(crop) = crop {
        page.set("CropBox", rect(crop));
    }
    if rotate != 0 {
        page.set("Rotate", Object::Integer(rotate));
    }
    let page_id = doc.add_object(page);
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save the exhibit");
    out
}

fn details() -> EvidenceDetails {
    EvidenceDetails {
        title: "Delil Listesi".into(),
        case_reference: "2026/123 E.".into(),
        court: String::new(),
        submitted_by: String::new(),
        language: "tr".into(),
    }
}

fn bundle_of(bytes: Vec<u8>) -> (Vec<u8>, u32) {
    let files = vec![EvidenceFile { label: "Ek-1".into(), file_name: "ek-1.pdf".into(), bytes }];
    let (bundle, manifest) = build_evidence_bundle(files, &details()).expect("bundle");
    // One cover page and one index page, then the exhibit's own pages.
    let exhibit_page = manifest.entries[0].first_page;
    (bundle, exhibit_page)
}

/// The text placement matrices of one page, in page space: `Tm` gives the text
/// matrix, `Td` moves it, and the content's `cm` composes on the left.
fn text_anchors(bundle: &[u8], page: u32) -> Vec<(f64, f64)> {
    let doc = Document::load_mem(bundle).expect("reload the bundle");
    let page_id = *doc.get_pages().get(&page).expect("the page");
    let content = String::from_utf8_lossy(&doc.get_page_content(page_id)).into_owned();
    let mut ctm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut stack: Vec<[f64; 6]> = Vec::new();
    let mut anchors = Vec::new();
    for line in content.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some((operator, operands)) = tokens.split_last() else { continue };
        let values: Vec<f64> = operands.iter().filter_map(|token| token.parse().ok()).collect();
        match *operator {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
            "cm" if values.len() == 6 => {
                // `cm` puts the new matrix inside the current one: the content
                // that follows is drawn in the new space, then transformed.
                let m = [values[0], values[1], values[2], values[3], values[4], values[5]];
                ctm = compose(ctm, m);
            }
            "BT" => tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            "Tm" if values.len() == 6 => tm = [values[0], values[1], values[2], values[3], values[4], values[5]],
            "Td" if values.len() == 2 => {
                tm = compose([1.0, 0.0, 0.0, 1.0, values[0], values[1]], tm);
            }
            "Tj" | "TJ" => {
                let combined = compose(ctm, tm);
                anchors.push((combined[4], combined[5]));
            }
            _ => {}
        }
    }
    anchors
}

/// `left` applied after `right`.
fn compose(left: [f64; 6], right: [f64; 6]) -> [f64; 6] {
    [
        left[0] * right[0] + left[2] * right[1],
        left[1] * right[0] + left[3] * right[1],
        left[0] * right[2] + left[2] * right[3],
        left[1] * right[2] + left[3] * right[3],
        left[0] * right[4] + left[2] * right[5] + left[4],
        left[1] * right[4] + left[3] * right[5] + left[5],
    ]
}

#[track_caller]
fn assert_near(actual: (f64, f64), expected: (f64, f64), what: &str) {
    let close = (actual.0 - expected.0).abs() <= 0.01 && (actual.1 - expected.1).abs() <= 0.01;
    assert!(close, "{what}: expected {expected:?}, got {actual:?}");
}

#[test]
fn the_exhibit_label_follows_the_page_box_origin() {
    let plain = bundle_of(exhibit([0.0, 0.0, 612.0, 792.0], None, 0));
    let shifted = bundle_of(exhibit([100.0, 200.0, 712.0, 992.0], None, 0));
    let a = text_anchors(&plain.0, plain.1);
    let b = text_anchors(&shifted.0, shifted.1);
    // Each exhibit page carries its label (top right) and a footer.
    assert_eq!(a.len(), 2, "label and footer on the exhibit page: {:?}", a);
    assert_eq!(b.len(), 2, "label and footer on the exhibit page: {:?}", b);
    assert_eq!(a.len(), b.len(), "both bundles stamp the same number of lines");
    for (before, after) in a.iter().zip(b.iter()) {
        assert_near((after.0 - before.0, after.1 - before.1), (100.0, 200.0), "both lines follow the box origin");
    }
}

#[test]
fn the_exhibit_label_sits_on_the_crop_box() {
    // The label is right-aligned at `width - 24` from the visible box's right
    // edge and `height - 22` from its top edge: on a 300x400 crop the numbers
    // move by the crop's corner plus the width and height the viewer lost.
    let plain = bundle_of(exhibit([0.0, 0.0, 612.0, 792.0], None, 0));
    let cropped = bundle_of(exhibit([0.0, 0.0, 612.0, 792.0], Some([50.0, 100.0, 350.0, 500.0]), 0));
    let a = text_anchors(&plain.0, plain.1);
    let b = text_anchors(&cropped.0, cropped.1);
    assert_near(
        (b[0].0 - a[0].0, b[0].1 - a[0].1),
        (50.0 + 300.0 - 612.0, 100.0 + 400.0 - 792.0),
        "label sits on the crop box",
    );
}

#[test]
fn a_rotated_exhibit_keeps_the_label_on_the_page() {
    // /Rotate 90 turns the 612x792 page into a 792x612 view, so the label's
    // display anchor (width - 24, height - 22) = (768, 590) lands at page
    // (612 - 590, 768 - 24 - 16.02)... worked out straight from the rotation
    // (ISO 32000-1 7.7.3.3): the display's x runs along the page's y and its y
    // runs against the page's x.
    let rotated = bundle_of(exhibit([0.0, 0.0, 612.0, 792.0], None, 90));
    let anchors = text_anchors(&rotated.0, rotated.1);
    assert!(!anchors.is_empty(), "the exhibit page carries its label");
    let label = anchors[0];
    // The label is 22 points below the top edge of the 792-point-tall view, so
    // its page x is 612 - 590 = 22; its anchor sits 768 points along the view.
    assert!((label.0 - 22.0).abs() < 12.0, "the label stays inside the page near its left edge: {label:?}");
    assert!(label.1 > 700.0 && label.1 < 792.0, "the label sits near the top of the unrotated page: {label:?}");
}

#[test]
fn every_exhibit_page_carries_one_label() {
    // Two exhibits of two pages each: the labels follow the manifest's page
    // numbers, not the order the bytes happen to sit in.
    let files = vec![
        EvidenceFile {
            label: "Ek-1".into(),
            file_name: "ek-1.pdf".into(),
            bytes: exhibit([0.0, 0.0, 612.0, 792.0], None, 0),
        },
        EvidenceFile {
            label: "Ek-2".into(),
            file_name: "ek-2.pdf".into(),
            bytes: exhibit([0.0, 0.0, 612.0, 792.0], None, 0),
        },
    ];
    let (bundle, manifest) = build_evidence_bundle(files, &details()).expect("bundle");
    assert_eq!(manifest.entries.len(), 2);
    assert_eq!(manifest.total_pages, 4, "one cover, one index, one page per exhibit");
    for entry in &manifest.entries {
        let anchors = text_anchors(&bundle, entry.first_page);
        assert_eq!(anchors.len(), 2, "exhibit {} gets its label and footer", entry.number);
    }
    let cover = text_anchors(&bundle, 1);
    assert!(!cover.is_empty(), "the cover page has its own text");
}

//! N-up and booklet imposition (v4.6.0).

mod common;

use common::*;
use lopdf::{Document, Object};
use pdfcore::docutil::OverwritePolicy;
use pdfcore::nup::{nup_pdf, nup_pdf_bytes, NupOptions};
use pdfcore::progress::CancelToken;
use std::path::Path;

/// Decoded content stream of an output page: N-up sheets keep only the
/// placement operators there, the source artwork lives in Form XObjects.
fn page_content(path: &Path, page: u32) -> String {
    let doc = Document::load(path).expect("load output");
    let pages = doc.get_pages();
    let id = *pages.get(&page).expect("page exists");
    String::from_utf8_lossy(&doc.get_page_content(id)).to_string()
}

/// Form names in the order the page paints them (`/FmN Do`).
fn paint_sequence(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let name = line.trim().strip_suffix(" Do")?;
            name.strip_prefix('/').map(str::to_string)
        })
        .collect()
}

/// The `cm` operands of a sheet content stream, in order.
fn paint_matrices(content: &str) -> Vec<[f64; 6]> {
    content
        .lines()
        .filter_map(|line| {
            let operands = line.trim().strip_suffix(" cm")?;
            let values: Vec<f64> =
                operands.split_whitespace().map(|value| value.parse().ok()).collect::<Option<_>>()?;
            (values.len() == 6).then(|| [values[0], values[1], values[2], values[3], values[4], values[5]])
        })
        .collect()
}

/// The axis-aligned rectangle a placement matrix gives a `w` x `h` page (the
/// matrix composes the source rotation, so its `e`/`f` are not the rect's
/// origin for rotated pages).
fn placed_rect(matrix: &[f64; 6], w: f64, h: f64) -> [f64; 4] {
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
        .map(|(x, y)| (matrix[0] * x + matrix[2] * y + matrix[4], matrix[1] * x + matrix[3] * y + matrix[5]));
    let min_x = corners.iter().map(|c| c.0).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|c| c.0).fold(f64::MIN, f64::max);
    let min_y = corners.iter().map(|c| c.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x - min_x, max_y - min_y]
}

/// The rectangle of the first `re S` frame, when the sheet draws one.
fn border_rect(content: &str) -> Option<[f64; 4]> {
    content.lines().find_map(|line| {
        let operands = line.trim().strip_suffix(" re S")?;
        let values: Vec<f64> = operands.split_whitespace().map(|value| value.parse().ok()).collect::<Option<_>>()?;
        (values.len() == 4).then(|| [values[0], values[1], values[2], values[3]])
    })
}

/// Text of an output page through pdfium, when the render engine is present.
/// Form XObject text is invisible to lopdf's own `extract_text`.
fn pdfium_text(path: &Path, page: u32) -> Option<String> {
    pdfcore::render::extract_page_text(path, None, page).ok().filter(|text| !text.trim().is_empty())
}

fn nup(input: &Path, output: &Path, options: &NupOptions) -> std::path::PathBuf {
    nup_pdf(input, output, options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new()).expect("nup_pdf")
}

#[test]
fn two_up_and_four_up_keep_reading_order() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "NUP", "N-up sample"), &input);

    let out2 = dir.path("two-up.pdf");
    let options = NupOptions::default();
    let result = nup(&input, &out2, &options);
    assert_eq!(result, out2);
    assert_eq!(page_count(&out2), 2);
    assert_eq!(pdfcore::info::pdf_info(&out2, None).expect("info").page_count, 2);
    // Default options: source size (A4 portrait) on a landscape sheet.
    let mb = media_box(&out2, 1);
    assert!((mb[2] - 841.89).abs() < 0.5 && (mb[3] - 595.28).abs() < 0.5, "landscape A4 sheet expected, got {mb:?}");
    assert_eq!(paint_sequence(&page_content(&out2, 1)), ["Fm0", "Fm1"]);
    assert_eq!(paint_sequence(&page_content(&out2, 2)), ["Fm2", "Fm3"]);
    if let Some(text) = pdfium_text(&out2, 1) {
        assert!(text.contains("NUP page 1") && text.contains("NUP page 2"), "sheet text: {text}");
    }
    if let Some(text) = pdfium_text(&out2, 2) {
        assert!(text.contains("NUP page 3") && text.contains("NUP page 4"), "sheet text: {text}");
    }

    let out4 = dir.path("four-up.pdf");
    let options4 = NupOptions { per_sheet: 4, ..Default::default() };
    nup(&input, &out4, &options4);
    assert_eq!(page_count(&out4), 1);
    assert_eq!(paint_sequence(&page_content(&out4, 1)), ["Fm0", "Fm1", "Fm2", "Fm3"]);
    if let Some(text) = pdfium_text(&out4, 1) {
        for page in 1..=4 {
            assert!(text.contains(&format!("NUP page {page}")), "missing page {page} in: {text}");
        }
    }
}

#[test]
fn booklet_orders_pages_for_saddle_stitch() {
    let dir = TestDir::new();
    let input = dir.path("eight.pdf");
    write_doc(&mut build_text_doc(8, "BK", "Booklet sample"), &input);

    let out = dir.path("booklet.pdf");
    let options = NupOptions { booklet: true, ..Default::default() };
    nup(&input, &out, &options);

    // Eight pages = two physical sheets, front and back side each.
    assert_eq!(page_count(&out), 4);
    let expected: [(&str, &str); 4] = [("Fm7", "Fm0"), ("Fm1", "Fm6"), ("Fm5", "Fm2"), ("Fm3", "Fm4")];
    for (index, (left, right)) in expected.iter().enumerate() {
        let content = page_content(&out, index as u32 + 1);
        assert_eq!(paint_sequence(&content), [*left, *right], "sheet side {}", index + 1);
    }
    if let Some(text) = pdfium_text(&out, 1) {
        assert!(text.contains("BK page 8") && text.contains("BK page 1"), "front side text: {text}");
    }
    if let Some(text) = pdfium_text(&out, 2) {
        assert!(text.contains("BK page 2") && text.contains("BK page 7"), "back side text: {text}");
    }
}

#[test]
fn page_selection_builds_a_single_sheet_from_the_chosen_pages() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "SEL", "Selection sample"), &input);

    let out = dir.path("selected.pdf");
    let options = NupOptions { pages: vec![2, 3], ..Default::default() };
    nup(&input, &out, &options);

    assert_eq!(page_count(&out), 1);
    assert_eq!(paint_sequence(&page_content(&out, 1)), ["Fm0", "Fm1"]);
    if let Some(text) = pdfium_text(&out, 1) {
        assert!(text.contains("SEL page 2") && text.contains("SEL page 3"), "sheet text: {text}");
        assert!(!text.contains("SEL page 1") && !text.contains("SEL page 4"), "unselected pages leaked: {text}");
    }
}

#[test]
fn invalid_options_and_page_ranges_are_rejected() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "BAD", "Validation sample"), &input);
    let bytes = std::fs::read(&input).expect("read input");

    let bad_grid = NupOptions { per_sheet: 3, ..Default::default() };
    assert!(matches!(nup_pdf_bytes(&bytes, &bad_grid), Err(pdfcore::PdfError::InvalidInput(_))));

    let booklet_four = NupOptions { per_sheet: 4, booklet: true, ..Default::default() };
    assert!(matches!(nup_pdf_bytes(&bytes, &booklet_four), Err(pdfcore::PdfError::InvalidInput(_))));

    let out_of_range = NupOptions { pages: vec![5], ..Default::default() };
    assert!(matches!(nup_pdf_bytes(&bytes, &out_of_range), Err(pdfcore::PdfError::RangeOutOfBounds)));

    let zero_page = NupOptions { pages: vec![0], ..Default::default() };
    assert!(matches!(nup_pdf_bytes(&bytes, &zero_page), Err(pdfcore::PdfError::RangeOutOfBounds)));

    // The path API must not leave an output behind for a rejected selection.
    let out = dir.path("bad.pdf");
    assert!(nup_pdf(&input, &out, &out_of_range, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
        .is_err());
    assert!(!out.exists(), "a rejected selection must not create an output");
}

#[test]
fn margin_and_gutter_keep_each_page_inside_the_margins() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "MRG", "Margin sample"), &input);

    let out = dir.path("margins.pdf");
    let options = NupOptions { margin_pt: 36.0, gutter_pt: 24.0, ..Default::default() };
    nup(&input, &out, &options);

    let (sheet_w, sheet_h) = (841.89, 595.28);
    let (page_w, page_h) = (595.28, 841.89);
    let content = page_content(&out, 1);
    let matrices = paint_matrices(&content);
    assert_eq!(matrices.len(), 2, "one placement matrix per placed page");
    let scale = matrices[0][0];
    assert!(scale > 0.0);
    let rects: Vec<[f64; 4]> = matrices.iter().map(|matrix| placed_rect(matrix, page_w, page_h)).collect();
    for (matrix, rect) in matrices.iter().zip(&rects) {
        assert!(matrix[1].abs() < 1e-6 && matrix[2].abs() < 1e-6 && (matrix[3] - scale).abs() < 1e-6);
        // The printed matrices carry 4 decimals, hence the 0.1pt tolerance.
        assert!(rect[0] >= 36.0 - 0.1, "left edge respects the margin: {matrix:?} -> {rect:?}");
        assert!(rect[1] >= 36.0 - 0.1, "bottom edge respects the margin: {matrix:?} -> {rect:?}");
        assert!(rect[0] + rect[2] <= sheet_w - 36.0 + 0.1, "right edge respects the margin: {matrix:?} -> {rect:?}");
        assert!(rect[1] + rect[3] <= sheet_h - 36.0 + 0.1, "top edge respects the margin: {matrix:?} -> {rect:?}");
    }
    // The gutter separates the columns: the second page starts one cell plus
    // one gutter to the right of the first.
    let cell_w = (sheet_w - 2.0 * 36.0 - 24.0) / 2.0;
    assert!((rects[1][0] - rects[0][0] - (cell_w + 24.0)).abs() < 0.1, "gutter must separate the columns");
    assert!(rects[0][0] < rects[1][0], "reading order is left to right");
}

#[test]
fn border_draws_a_light_frame_only_when_requested() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "BRD", "Border sample"), &input);

    let plain = dir.path("plain.pdf");
    nup(&input, &plain, &NupOptions::default());
    let plain_content = page_content(&plain, 1);
    assert!(!plain_content.contains(" re S"), "no frame without border: {plain_content}");
    assert!(border_rect(&plain_content).is_none());

    let bordered = dir.path("bordered.pdf");
    nup(&input, &bordered, &NupOptions { border: true, ..Default::default() });
    let content = page_content(&bordered, 1);
    assert!(content.contains(" re S"), "frame expected: {content}");
    let rect = border_rect(&content).expect("frame rectangle");
    let matrices = paint_matrices(&content);
    let expected = placed_rect(&matrices[0], 595.28, 841.89);
    for index in 0..4 {
        assert!((rect[index] - expected[index]).abs() < 0.1, "frame {rect:?} must match the placed page {expected:?}");
    }
}

#[test]
fn sheet_size_and_orientation_are_honoured() {
    let dir = TestDir::new();
    let input = dir.path("four.pdf");
    write_doc(&mut build_text_doc(4, "SZ", "Size sample"), &input);

    let a4_portrait = dir.path("a4-portrait.pdf");
    let options = NupOptions { page_size: "a4".into(), orientation: "portrait".into(), ..Default::default() };
    nup(&input, &a4_portrait, &options);
    let mb = media_box(&a4_portrait, 1);
    assert!((mb[2] - 595.28).abs() < 0.5 && (mb[3] - 841.89).abs() < 0.5, "portrait A4 expected, got {mb:?}");

    let letter_landscape = dir.path("letter-landscape.pdf");
    let options = NupOptions { page_size: "letter".into(), orientation: "landscape".into(), ..Default::default() };
    nup(&input, &letter_landscape, &options);
    let mb = media_box(&letter_landscape, 1);
    assert!((mb[2] - 792.0).abs() < 0.5 && (mb[3] - 612.0).abs() < 0.5, "landscape letter expected, got {mb:?}");
}

#[test]
fn a_rotated_source_page_is_pre_rotated_into_its_cell() {
    let dir = TestDir::new();
    let input = dir.path("rotated.pdf");
    let mut doc = build_text_doc(2, "ROT", "Rotated sample");
    let first = *doc.get_pages().get(&1).expect("page 1");
    doc.get_object_mut(first).unwrap().as_dict_mut().unwrap().set("Rotate", Object::Integer(90));
    write_doc(&mut doc, &input);

    let out = dir.path("rotated-out.pdf");
    nup(&input, &out, &NupOptions { pages: vec![1], ..Default::default() });

    let matrices = paint_matrices(&page_content(&out, 1));
    let matrix = matrices[0];
    // A page -> display transform for 90 degrees is [0 -1 1 0 0 w]; scaled by
    // the fit factor this must show up as a clockwise quarter turn.
    assert!(matrix[0].abs() < 1e-6 && matrix[3].abs() < 1e-6, "rotation terms expected: {matrix:?}");
    assert!(matrix[1] < 0.0 && matrix[2] > 0.0, "clockwise quarter turn expected: {matrix:?}");
    assert!((matrix[1] + matrix[2]).abs() < 1e-6, "rigid rotation keeps |b| = |c|: {matrix:?}");
    // The rotated page is placed as its displayed (landscape) self and fits.
    let rect = placed_rect(&matrix, 595.28, 841.89);
    assert!(rect[2] > rect[3], "the rotated page must be placed landscape: {rect:?}");
    assert!(rect[0] >= 18.0 - 0.1 && rect[1] >= 18.0 - 0.1, "inside the margins: {rect:?}");
    assert!(rect[0] + rect[2] <= 841.89 - 18.0 + 0.1, "inside the margins: {rect:?}");
    assert!(rect[1] + rect[3] <= 595.28 - 18.0 + 0.1, "inside the margins: {rect:?}");
}

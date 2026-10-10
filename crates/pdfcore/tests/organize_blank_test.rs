//! Blank page insertion through the page plan (v4.6.0).

mod common;

use common::*;
use pdfcore::docutil::{OverwritePolicy, PagePlanItem};
use pdfcore::organize::apply_page_plan;

#[test]
fn blank_page_between_two_sources_keeps_the_order_and_size() {
    let dir = TestDir::new();
    let input = dir.path("blank.pdf");
    write_doc(&mut build_text_doc(2, "BLK", "Blank sample"), &input);

    let plan = vec![
        PagePlanItem { source_page: 1, ..Default::default() },
        PagePlanItem { blank: true, ..Default::default() },
        PagePlanItem { source_page: 2, ..Default::default() },
    ];
    let out = dir.path("blanked.pdf");
    apply_page_plan(&input, &plan, &out, OverwritePolicy::Replace, None).unwrap();

    assert_eq!(page_count(&out), 3);
    assert!(page_text(&out, 1).contains("BLK page 1"));
    assert_eq!(page_text(&out, 2).trim(), "", "the inserted page must be empty");
    assert!(page_text(&out, 3).contains("BLK page 2"));

    let reference = media_box(&out, 1);
    let blank = media_box(&out, 2);
    assert!((blank[2] - reference[2]).abs() < 0.01, "blank width must match the reference page");
    assert!((blank[3] - reference[3]).abs() < 0.01, "blank height must match the reference page");
    assert_eq!(page_rotation(&out, 2), 0);
}

#[test]
fn blank_page_honours_an_explicit_size() {
    let dir = TestDir::new();
    let input = dir.path("sized.pdf");
    write_doc(&mut build_text_doc(1, "SIZED", "Sized sample"), &input);

    let plan = vec![
        PagePlanItem { source_page: 1, ..Default::default() },
        PagePlanItem { blank: true, width_pt: Some(300.0), height_pt: Some(400.0), ..Default::default() },
    ];
    let out = dir.path("sized-out.pdf");
    apply_page_plan(&input, &plan, &out, OverwritePolicy::Replace, None).unwrap();

    assert_eq!(page_count(&out), 2);
    let blank = media_box(&out, 2);
    assert!((blank[2] - 300.0).abs() < 0.01);
    assert!((blank[3] - 400.0).abs() < 0.01);
    assert_eq!(page_text(&out, 2).trim(), "");
}

#[test]
fn all_blank_plan_produces_a_blank_document() {
    // An all-blank plan is deliberately accepted (unlike a truly empty plan):
    // it still names at least one page, and every size falls back to A4.
    let dir = TestDir::new();
    let input = dir.path("source.pdf");
    write_doc(&mut build_text_doc(1, "SRC", "Source sample"), &input);

    let plan = vec![
        PagePlanItem { blank: true, ..Default::default() },
        PagePlanItem { blank: true, rotation_delta: 90, ..Default::default() },
    ];
    let out = dir.path("all-blank.pdf");
    apply_page_plan(&input, &plan, &out, OverwritePolicy::Replace, None).unwrap();

    assert_eq!(page_count(&out), 2);
    assert_eq!(page_text(&out, 1).trim(), "");
    assert_eq!(page_text(&out, 2).trim(), "");
    let first = media_box(&out, 1);
    assert!((first[2] - 595.28).abs() < 0.01 && (first[3] - 841.89).abs() < 0.01, "A4 fallback expected");
    assert_eq!(page_rotation(&out, 2), 90);
}

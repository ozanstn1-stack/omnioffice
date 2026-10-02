mod common;

use common::*;
use lopdf::{dictionary, Object};
use pdfcore::docutil::{OverwritePolicy, PagePlanItem};
use pdfcore::merge::{merge_documents, merge_files, MergeOptions};
use pdfcore::metadata::{edit_metadata_file, read_metadata_from_file, PdfMetadata};
use pdfcore::organize::{apply_page_plan, delete_pages, extract_pages, rotate_pages, split_pdf};
use pdfcore::pagelayout::{crop_pages, resize_pages, CropItem, ResizeOptions};
use pdfcore::pages::{parse_page_selection, SplitMode};
use pdfcore::progress::CancelToken;

fn setup(label: &str, pages: u32) -> (TestDir, std::path::PathBuf) {
    let dir = TestDir::new();
    let path = dir.path(&format!("{label}.pdf"));
    write_doc(&mut build_text_doc(pages, label, &format!("{label} title")), &path);
    (dir, path)
}

#[test]
fn merge_preserves_order_pages_and_metadata() {
    let dir = TestDir::new();
    let a = dir.path("a.pdf");
    let b = dir.path("b.pdf");
    write_doc(&mut build_text_doc(3, "AAA", "Document A"), &a);
    write_doc(&mut build_text_doc(2, "BBB", "Document B"), &b);

    let out = dir.path("merged.pdf");
    let cancel = CancelToken::new();
    let (path, pages) = merge_files(
        &[a.clone(), b.clone()],
        &out,
        &MergeOptions { preserve_metadata: true },
        OverwritePolicy::Replace,
        &no_progress,
        &cancel,
    )
    .expect("merge succeeds");

    assert_eq!(pages, 5);
    assert_eq!(page_count(&path), 5);
    assert!(page_text(&path, 1).contains("AAA page 1"));
    assert!(page_text(&path, 3).contains("AAA page 3"));
    assert!(page_text(&path, 4).contains("BBB page 1"));
    assert!(page_text(&path, 5).contains("BBB page 2"));
    let meta = read_metadata_from_file(&path).unwrap();
    assert_eq!(meta.title, "Document A");
}

#[test]
fn merge_keeps_nonzero_generations_without_collisions() {
    // A PDF updated in place can legally hold the same object number with two
    // generations. Remapping must keep them apart; resetting every generation
    // to 0 (a suggested "fix") would collapse them onto one key and lose an
    // object. This test pins the safe behaviour.
    let mut first = build_text_doc(1, "GEN", "Generations");
    first.objects.insert(
        (2, 7),
        Object::Dictionary(dictionary! {
            "Type" => "Metadata",
            "Note" => Object::string_literal("generation-7"),
        }),
    );
    let second = build_text_doc(1, "PLAIN", "Plain");

    let merged = merge_documents(
        vec![("gen.pdf".to_string(), first), ("plain.pdf".to_string(), second)],
        &MergeOptions::default(),
    )
    .expect("merge succeeds");

    assert!(merged.objects.keys().any(|id| id.1 == 7), "the non-zero generation must survive the remap");
    let notes = merged
        .objects
        .values()
        .filter_map(|object| {
            object.as_dict().ok().and_then(|dict| dict.get(b"Note").ok()).and_then(|note| note.as_str().ok())
        })
        .filter(|note| *note == b"generation-7")
        .count();
    assert_eq!(notes, 1, "the hand-inserted object must not be overwritten");
}

#[test]
fn merge_requires_two_inputs() {
    let dir = TestDir::new();
    let a = dir.path("a.pdf");
    write_doc(&mut build_text_doc(1, "Solo", "Solo"), &a);
    let cancel = CancelToken::new();
    let result = merge_files(
        &[a],
        &dir.path("out.pdf"),
        &MergeOptions::default(),
        OverwritePolicy::Replace,
        &no_progress,
        &cancel,
    );
    assert!(result.is_err());
}

#[test]
fn delete_and_extract_pages() {
    let (dir, input) = setup("del", 6);

    let without = dir.path("without.pdf");
    delete_pages(&input, &[2, 5], &without, OverwritePolicy::Replace, None).unwrap();
    assert_eq!(page_count(&without), 4);
    assert!(page_text(&without, 1).contains("del page 1"));
    assert!(page_text(&without, 2).contains("del page 3"));
    assert!(page_text(&without, 4).contains("del page 6"));

    let extracted = dir.path("extract.pdf");
    let pages = parse_page_selection("1,3,5-6", 6).unwrap();
    extract_pages(&input, &pages, &extracted, OverwritePolicy::Replace, None).unwrap();
    assert_eq!(page_count(&extracted), 4);
    assert!(page_text(&extracted, 1).contains("del page 1"));
    assert!(page_text(&extracted, 2).contains("del page 3"));
    assert!(page_text(&extracted, 3).contains("del page 5"));
}

#[test]
fn deleting_every_page_is_rejected() {
    let (dir, input) = setup("alldel", 3);
    let result = delete_pages(&input, &[1, 2, 3], &dir.path("out.pdf"), OverwritePolicy::Replace, None);
    assert!(result.is_err());
}

#[test]
fn reorder_duplicate_and_rotate_via_plan() {
    let (dir, input) = setup("plan", 4);
    let plan = vec![
        PagePlanItem { source_page: 3, rotation_delta: 0 },
        PagePlanItem { source_page: 1, rotation_delta: 90 },
        PagePlanItem { source_page: 1, rotation_delta: 180 },
        PagePlanItem { source_page: 4, rotation_delta: 0 },
    ];
    let out = dir.path("plan.pdf");
    apply_page_plan(&input, &plan, &out, OverwritePolicy::Replace, None).unwrap();
    assert_eq!(page_count(&out), 4);
    assert!(page_text(&out, 1).contains("plan page 3"));
    assert!(page_text(&out, 2).contains("plan page 1"));
    assert!(page_text(&out, 3).contains("plan page 1"));
    assert!(page_text(&out, 4).contains("plan page 4"));
    assert_eq!(page_rotation(&out, 2), 90);
    assert_eq!(page_rotation(&out, 3), 180);
    assert_eq!(page_rotation(&out, 1), 0);
}

#[test]
fn rotate_pages_selection() {
    let (dir, input) = setup("rot", 3);
    let out = dir.path("rotated.pdf");
    rotate_pages(&input, &[1, 3], 270, &out, OverwritePolicy::Replace, None).unwrap();
    assert_eq!(page_rotation(&out, 1), 270);
    assert_eq!(page_rotation(&out, 2), 0);
    assert_eq!(page_rotation(&out, 3), 270);
}

#[test]
fn split_modes_produce_expected_files() {
    let (dir, input) = setup("split", 5);
    let cancel = CancelToken::new();

    let every = dir.path("every");
    let parts =
        split_pdf(&input, &SplitMode::EveryN { n: 2 }, &every, OverwritePolicy::Replace, None, &no_progress, &cancel)
            .unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(page_count(std::path::Path::new(&parts[0].path)), 2);
    assert_eq!(page_count(std::path::Path::new(&parts[2].path)), 1);

    let individual = dir.path("single");
    let parts =
        split_pdf(&input, &SplitMode::Individual, &individual, OverwritePolicy::Replace, None, &no_progress, &cancel)
            .unwrap();
    assert_eq!(parts.len(), 5);
    for part in &parts {
        assert_eq!(page_count(std::path::Path::new(&part.path)), 1);
    }

    let at_pages = dir.path("at");
    let parts = split_pdf(
        &input,
        &SplitMode::AtPages { pages: vec![2, 4] },
        &at_pages,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    )
    .unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(page_count(std::path::Path::new(&parts[0].path)), 1);

    let ranges = dir.path("ranges");
    let parts = split_pdf(
        &input,
        &SplitMode::Ranges { ranges: vec!["1-3".into(), "4-5".into()] },
        &ranges,
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    )
    .unwrap();
    assert_eq!(parts.len(), 2);
    assert!(page_text(std::path::Path::new(&parts[1].path), 1).contains("split page 4"));
}

#[test]
fn resize_to_letter_scales_content() {
    let (dir, input) = setup("resize", 2);
    let out = dir.path("resized.pdf");
    let options = ResizeOptions {
        page_size: "letter".into(),
        mode: "fit".into(),
        orientation: "auto".into(),
        ..Default::default()
    };
    resize_pages(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new()).unwrap();
    let box_ = media_box(&out, 1);
    assert!((box_[2] - 612.0).abs() < 1.0);
    assert!((box_[3] - 792.0).abs() < 1.0);
    assert!(page_text(&out, 1).contains("resize page 1"));
}

#[test]
fn crop_sets_cropbox_only() {
    let (dir, input) = setup("crop", 1);
    let out = dir.path("cropped.pdf");
    let crops = vec![CropItem { page: 1, x: 50.0, y: 60.0, w: 300.0, h: 400.0 }];
    crop_pages(&input, &out, &crops, OverwritePolicy::Replace, None).unwrap();
    let crop = crop_box(&out, 1).expect("cropbox present");
    let media = media_box(&out, 1);
    assert!((media[2] - 595.28).abs() < 1.0);
    assert!(crop[2] > 300.0 && crop[2] < 400.0);
    assert!(crop[3] > media[3] - 200.0);
}

#[test]
fn metadata_edit_and_remove() {
    let (dir, input) = setup("meta", 1);
    let meta = PdfMetadata {
        title: "Rapor: şğüöç 2026".into(),
        author: "Test Yazarı".into(),
        keywords: "pdf, test, türkçe".into(),
        ..Default::default()
    };
    let edited = dir.path("meta-edited.pdf");
    edit_metadata_file(&input, &edited, &meta, false, OverwritePolicy::Replace, None).unwrap();
    let read_back = read_metadata_from_file(&edited).unwrap();
    assert_eq!(read_back.title, "Rapor: şğüöç 2026");
    assert_eq!(read_back.author, "Test Yazarı");

    let cleaned = dir.path("meta-removed.pdf");
    edit_metadata_file(&input, &cleaned, &PdfMetadata::default(), true, OverwritePolicy::Replace, None).unwrap();
    let empty = read_metadata_from_file(&cleaned).unwrap();
    assert!(empty.is_empty());
    // content must be untouched
    assert!(page_text(&cleaned, 1).contains("meta page 1"));
}

#[test]
fn info_reports_document_facts() {
    let (_dir, input) = setup("info", 3);
    let info = pdfcore::info::pdf_info(&input, None).unwrap();
    assert_eq!(info.page_count, 3);
    assert!(!info.encrypted);
    assert_eq!(info.file_name, "info.pdf");
    assert_eq!(info.title, "info title");
    if pdfcore::render::is_available() {
        assert!(info.has_text_layer);
        assert_eq!(info.page_geometries.len(), 3);
        let summary = pdfcore::info::summarize_sizes(&info.page_geometries);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].count, 3);
    }
}

#[test]
fn overwrite_policies_behave() {
    let (dir, input) = setup("ow", 1);
    let out = dir.path("target.pdf");
    write_doc(&mut build_text_doc(1, "existing", "existing"), &out);

    let error = extract_pages(&input, &[1], &out, OverwritePolicy::Error, None);
    assert!(matches!(error, Err(pdfcore::PdfError::OutputExists(_))));

    let replaced = extract_pages(&input, &[1], &out, OverwritePolicy::Replace, None).unwrap();
    assert!(page_text(&replaced, 1).contains("ow page 1"));

    let unique = extract_pages(&input, &[1], &out, OverwritePolicy::UniqueName, None).unwrap();
    assert_ne!(unique, out);
    assert!(unique.file_name().unwrap().to_string_lossy().contains("(1)"));
}

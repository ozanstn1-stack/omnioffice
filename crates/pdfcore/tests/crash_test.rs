//! Crash / hostile input tests: the app must fail in a controlled, friendly
//! way for broken inputs, locked files, cancellations and missing engines.

mod common;

use common::*;
use pdfcore::compress::{compress_pdf, CompressOptions};
use pdfcore::docutil::{load_document, OverwritePolicy};
use pdfcore::merge::{merge_files, MergeOptions};
use pdfcore::organize::{extract_pages, split_pdf};
use pdfcore::pages::SplitMode;
use pdfcore::progress::CancelToken;
use pdfcore::security::{protect_pdf, ProtectOptions};
use std::path::PathBuf;

#[test]
fn empty_file_is_rejected() {
    let dir = TestDir::new();
    let empty = dir.path("empty.pdf");
    std::fs::write(&empty, b"").unwrap();
    let result = load_document(&empty, None);
    assert!(matches!(result, Err(pdfcore::PdfError::InvalidPdf(_))));
}

#[test]
fn corrupt_pdf_is_rejected() {
    let dir = TestDir::new();
    let corrupt = dir.path("corrupt.pdf");
    std::fs::write(&corrupt, b"%PDF-1.7\nthis is not really a pdf\n%%EOF").unwrap();
    let result = load_document(&corrupt, None);
    assert!(result.is_err(), "corrupt input must not load");
}

#[test]
fn missing_file_gives_not_found() {
    let dir = TestDir::new();
    let result = load_document(&dir.path("nope.pdf"), None);
    assert!(matches!(result, Err(pdfcore::PdfError::NotFound(_))));
}

#[test]
fn password_protected_without_password_asks_for_it() {
    let dir = TestDir::new();
    let input = dir.path("secret.pdf");
    write_doc(&mut build_text_doc(1, "secret", "secret"), &input);
    let protected = dir.path("secret-protected.pdf");
    protect_pdf(
        &input,
        &protected,
        &ProtectOptions {
            user_password: "a".into(),
            owner_password: "b".into(),
            allow_printing: false,
            allow_copying: false,
            allow_editing: false,
            allow_commenting: false,
        },
        OverwritePolicy::Replace,
        None,
    )
    .unwrap();
    let result = load_document(&protected, None);
    assert!(matches!(result, Err(pdfcore::PdfError::PasswordRequired)));
}

#[test]
fn fake_extension_is_treated_as_invalid_pdf() {
    let dir = TestDir::new();
    let fake = dir.path("not-a-pdf.pdf");
    std::fs::write(&fake, b"MZ\x90\x00 this is an executable, not a pdf").unwrap();
    let result = load_document(&fake, None);
    assert!(result.is_err());
}

#[test]
fn many_page_document_handles_split_and_merge() {
    let dir = TestDir::new();
    let big = dir.path("big.pdf");
    write_doc(&mut build_text_doc(120, "big", "big doc"), &big);
    assert_eq!(page_count(&big), 120);

    let cancel = CancelToken::new();
    let parts = split_pdf(
        &big,
        &SplitMode::EveryN { n: 25 },
        &dir.path("chunks"),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    )
    .unwrap();
    assert_eq!(parts.len(), 5);

    let merged = dir.path("merged-back.pdf");
    let inputs: Vec<PathBuf> = parts.iter().map(|p| PathBuf::from(&p.path)).collect();
    let (path, pages) = merge_files(
        &inputs,
        &merged,
        &MergeOptions { preserve_metadata: false },
        OverwritePolicy::Replace,
        &no_progress,
        &cancel,
    )
    .unwrap();
    assert_eq!(pages, 120);
    assert!(page_text(&path, 120).contains("big page 120"));
}

#[test]
fn cancelled_operations_stop_early() {
    let dir = TestDir::new();
    let input = dir.path("cancel.pdf");
    write_doc(&mut build_text_doc(30, "cancel", "cancel"), &input);
    let cancel = CancelToken::new();
    cancel.cancel();

    let split = split_pdf(
        &input,
        &SplitMode::Individual,
        &dir.path("out"),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    );
    assert!(matches!(split, Err(pdfcore::PdfError::Cancelled)));

    let extract = extract_pages(&input, &[1], &dir.path("x.pdf"), OverwritePolicy::Replace, None);
    assert!(extract.is_ok(), "without a token, operations complete normally");

    let compress = compress_pdf(
        &input,
        &dir.path("c.pdf"),
        &CompressOptions::default(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    );
    assert!(matches!(compress, Err(pdfcore::PdfError::Cancelled)));
}

// Exclusive share-mode locks only exist on Windows.
#[cfg(windows)]
#[test]
fn locked_output_is_reported_as_file_locked() {
    let dir = TestDir::new();
    let input = dir.path("locked.pdf");
    write_doc(&mut build_text_doc(1, "locked", "locked"), &input);

    let target = dir.path("target.pdf");
    write_doc(&mut build_text_doc(1, "target", "target"), &target);
    let original_bytes = std::fs::read(&target).unwrap();
    // Simulate a real-world exclusive lock (e.g. the file is open in a PDF
    // reader that does not share write access).
    let lock = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(0).open(&target).unwrap()
    };
    // A locked target must surface a controlled error, not a panic.
    let result = extract_pages(&input, &[1], &target, OverwritePolicy::Replace, None);
    match result {
        Err(pdfcore::PdfError::FileLocked(_)) | Err(pdfcore::PdfError::PermissionDenied(_)) => {}
        Err(other) => panic!("expected a lock/permission error, got {other:?}"),
        Ok(_) => panic!("expected a failure while the file is locked"),
    }
    // The locked file must be untouched.
    drop(lock);
    assert_eq!(std::fs::read(&target).unwrap(), original_bytes);
}

#[test]
fn range_out_of_bounds_is_rejected() {
    let dir = TestDir::new();
    let input = dir.path("range.pdf");
    write_doc(&mut build_text_doc(3, "range", "range"), &input);
    let result = extract_pages(&input, &[5], &dir.path("out.pdf"), OverwritePolicy::Replace, None);
    assert!(matches!(result, Err(pdfcore::PdfError::RangeOutOfBounds)));
}

#[test]
fn error_codes_are_stable_for_the_ui() {
    let dir = TestDir::new();
    let missing = load_document(&dir.path("missing.pdf"), None).unwrap_err();
    assert_eq!(missing.code(), pdfcore::ErrorCode::NotFound);
    let empty = dir.path("empty.pdf");
    std::fs::write(&empty, b"").unwrap();
    let invalid = load_document(&empty, None).unwrap_err();
    assert_eq!(invalid.code(), pdfcore::ErrorCode::InvalidPdf);
    // Serialization shape the frontend expects.
    let json = serde_json::to_string(&invalid).unwrap();
    assert!(json.contains("\"code\":\"invalid_pdf\""));
    assert!(json.contains("\"message\""));
}

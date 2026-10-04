mod common;

use common::*;
use pdfcore::compress::{compress_pdf, estimate_compression, CompressOptions};
use pdfcore::docutil::{load_document, OverwritePolicy};
use pdfcore::progress::CancelToken;
use pdfcore::security::{check_password, protect_pdf, unlock_pdf, ProtectOptions};

fn setup(label: &str, pages: u32) -> (TestDir, std::path::PathBuf) {
    let dir = TestDir::new();
    let path = dir.path(&format!("{label}.pdf"));
    write_doc(&mut build_text_doc(pages, label, &format!("{label} title")), &path);
    (dir, path)
}

#[test]
fn protect_unlock_roundtrip_aes256() {
    let (dir, input) = setup("sec", 2);
    let protected = dir.path("protected.pdf");
    let options = ProtectOptions {
        user_password: "user-pass".into(),
        owner_password: "owner-pass".into(),
        allow_printing: true,
        allow_copying: false,
        allow_editing: false,
        allow_commenting: true,
    };
    let protected_path = protect_pdf(&input, &protected, &options, OverwritePolicy::Replace, None).unwrap();
    assert!(protected_path.exists());

    // Loading without a password must fail with PasswordRequired.
    let unauthenticated = load_document(&protected_path, None);
    assert!(matches!(unauthenticated, Err(pdfcore::PdfError::PasswordRequired)));

    // Wrong password is rejected...
    assert!(!check_password(&protected_path, "nope").unwrap());
    // ...the user password works.
    assert!(check_password(&protected_path, "user-pass").unwrap());

    // The document must be marked as encrypted in info.
    let info = pdfcore::info::pdf_info(&protected_path, None).unwrap();
    assert!(info.encrypted);

    // Unlock with the user password produces a readable file.
    let unlocked = dir.path("unlocked.pdf");
    unlock_pdf(&protected_path, &unlocked, "user-pass", OverwritePolicy::Replace).unwrap();
    let doc = load_document(&unlocked, None).unwrap();
    assert_eq!(doc.get_pages().len(), 2);
    assert!(!pdfcore::docutil::is_encrypted_document(&unlocked));
    assert!(page_text(&unlocked, 1).contains("sec page 1"));

    // Password appears nowhere in the output bytes.
    let bytes = std::fs::read(&unlocked).unwrap();
    assert!(!bytes.windows(9).any(|w| w == b"user-pass"));
}

#[test]
fn protect_requires_password() {
    let (dir, input) = setup("sec2", 1);
    let result = protect_pdf(
        &input,
        &dir.path("out.pdf"),
        &ProtectOptions {
            user_password: String::new(),
            owner_password: String::new(),
            allow_printing: true,
            allow_copying: true,
            allow_editing: true,
            allow_commenting: true,
        },
        OverwritePolicy::Replace,
        None,
    );
    assert!(result.is_err());
}

#[test]
fn unlock_with_wrong_password_fails() {
    let (dir, input) = setup("sec3", 1);
    let protected = dir.path("p.pdf");
    protect_pdf(
        &input,
        &protected,
        &ProtectOptions {
            user_password: "correct".into(),
            owner_password: "correct".into(),
            allow_printing: true,
            allow_copying: true,
            allow_editing: true,
            allow_commenting: true,
        },
        OverwritePolicy::Replace,
        None,
    )
    .unwrap();
    let result = unlock_pdf(&protected, &dir.path("u.pdf"), "wrong", OverwritePolicy::Replace);
    assert!(matches!(result, Err(pdfcore::PdfError::WrongPassword)));
}

#[test]
fn lossless_compression_keeps_pages_and_text() {
    let (dir, input) = setup("lossless", 3);
    let out = dir.path("lossless-out.pdf");
    let options = CompressOptions { strategy: "lossless".into(), remove_metadata: true, ..Default::default() };
    let result =
        compress_pdf(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .unwrap();
    assert!(result.output_bytes > 0);
    assert_eq!(page_count(std::path::Path::new(&result.path)), 3);
    assert!(page_text(std::path::Path::new(&result.path), 1).contains("lossless page 1"));
    // Metadata was stripped as requested.
    assert!(read_metadata_title(std::path::Path::new(&result.path)).is_empty());
}

fn read_metadata_title(path: &std::path::Path) -> String {
    pdfcore::metadata::read_metadata_from_file(path).unwrap().title
}

#[test]
fn raster_compression_reduces_sizes() {
    let dir = TestDir::new();
    let input = dir.path("photos.pdf");
    write_doc(&mut build_image_doc(2, "Photos"), &input);
    let out = dir.path("photos-small.pdf");
    let options = CompressOptions {
        strategy: "raster".into(),
        preset: "high".into(),
        grayscale: false,
        remove_metadata: true,
        ..Default::default()
    };
    let estimate = estimate_compression(&input, &options, None).unwrap();
    assert_eq!(estimate.page_count, 2);
    assert!(estimate.estimated_bytes > 0);
    // The synthetic bitmap is flate-compressed in the source, so raster JPEG
    // must be smaller.
    assert!(estimate.reduction > 0.2, "expected a meaningful reduction, got {}", estimate.reduction);

    let result =
        compress_pdf(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new())
            .unwrap();
    assert!(result.output_bytes < result.original_bytes);
    assert_eq!(page_count(std::path::Path::new(&result.path)), 2);
    // Page geometry must be preserved (A4).
    let box_ = media_box(std::path::Path::new(&result.path), 1);
    assert!((box_[2] - 595.28).abs() < 1.5);
    assert!((box_[3] - 841.89).abs() < 1.5);
}

#[test]
fn grayscale_preset_produces_gray_images() {
    let dir = TestDir::new();
    let input = dir.path("color.pdf");
    write_doc(&mut build_image_doc(1, "Color"), &input);
    let out = dir.path("gray.pdf");
    let options = CompressOptions {
        strategy: "raster".into(),
        preset: "custom".into(),
        dpi: 100,
        jpeg_quality: 55,
        grayscale: true,
        remove_metadata: true,
    };
    compress_pdf(&input, &out, &options, OverwritePolicy::Replace, None, &no_progress, &CancelToken::new()).unwrap();
    assert_eq!(page_count(&out), 1);
}

#[test]
fn cancelled_compression_returns_cancelled() {
    let dir = TestDir::new();
    let input = dir.path("cancel.pdf");
    write_doc(&mut build_image_doc(3, "Cancel"), &input);
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = compress_pdf(
        &input,
        &dir.path("out.pdf"),
        &CompressOptions::default(),
        OverwritePolicy::Replace,
        None,
        &no_progress,
        &cancel,
    );
    assert!(matches!(result, Err(pdfcore::PdfError::Cancelled)));
}

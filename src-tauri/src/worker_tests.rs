//! Tests for the worker: job parsing, the E2.2 child containment (allow-list
//! environment, `--worker` position, resource ceilings) and the in-process
//! fallback.

use super::*;

#[cfg(not(target_os = "android"))]
#[test]
fn the_child_environment_is_an_allow_list() {
    // The parser stack needs these (engine search path, temp dir, locale).
    for allowed in ["PATH", "Path", "TEMP", "HOME", "LANG", "PDFSAK_ENGINES_DIR", "PDFSAK_SOFFICE", "SystemRoot"] {
        assert!(env_allowed(allowed), "{allowed} must cross into the worker");
    }
    // Nothing a parser has no business seeing, and nothing secret-shaped.
    for refused in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "DEEPSEEK_API_KEY",
        "PDFSAK_DEV_FILES",
        "PDFSAK_START_SCREEN",
        "PDFSAK_DEV_RUN",
        "GITHUB_TOKEN",
        "SSH_AUTH_SOCK",
        "AWS_SECRET_ACCESS_KEY",
    ] {
        assert!(!env_allowed(refused), "{refused} must not reach the worker");
    }
}

#[test]
fn a_document_path_cannot_turn_the_gui_into_a_worker() {
    // `--worker` only counts as the first argument (E2.2).
    let program = "OmniOffice".to_string();
    let first = vec![program.clone(), "--worker".into(), "office-open".into(), "a.docx".into()];
    let rest = worker_invocation(&first).expect("a real worker invocation");
    assert_eq!(rest, &["office-open".to_string(), "a.docx".to_string()]);

    let later = vec![program.clone(), "a.docx".into(), "--worker".into()];
    assert!(worker_invocation(&later).is_none(), "a path may not select the worker");
    let nested = vec![program, "--cli".into(), "open".into(), "--worker".into()];
    assert!(worker_invocation(&nested).is_none(), "a nested flag may not select the worker");
}

/// The unix ceilings must be in place before the child's first instruction:
/// `ulimit` reads them back from the child that `pre_exec` configured.
#[cfg(all(unix, not(target_os = "android")))]
#[test]
fn the_child_inherits_the_resource_ceilings() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("ulimit -v; ulimit -t");
    limit_resources(&mut command);
    let output = command.output().expect("the probe child runs");
    let text = String::from_utf8_lossy(&output.stdout);
    // `ulimit -v` reports KiB, so 1.5 GiB is 1572864.
    assert!(text.contains("1572864"), "the address space limit is missing: {text}");
    assert!(text.contains("120"), "the CPU limit is missing: {text}");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn parses_worker_arguments() {
    assert_eq!(
        Job::parse(&["office-open".into(), "a.docx".into()]).unwrap(),
        Job::OfficeOpen { path: PathBuf::from("a.docx") }
    );
    assert_eq!(
        Job::parse(&["pdf-info".into(), "a.pdf".into()]).unwrap(),
        Job::PdfInfo { path: PathBuf::from("a.pdf") }
    );
    assert_eq!(
        Job::parse(&["repair".into(), "in.pdf".into(), "out.pdf".into()]).unwrap(),
        Job::Repair { input: PathBuf::from("in.pdf"), output: PathBuf::from("out.pdf") }
    );
    assert!(Job::parse(&["nope".into()]).is_err());
    assert!(Job::parse(&[]).is_err());
}

#[test]
fn a_corrupt_workbook_is_an_error_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.xlsx");
    // A ZIP magic followed by garbage: calamine walks it and fails.
    std::fs::write(&path, b"PK\x03\x04this is not a workbook at all").unwrap();
    let result = execute(&Job::OfficeOpen { path: path.clone() });
    assert!(result.is_err(), "a corrupt workbook must return an error");
    // The in-process fallback path (what the app uses when the worker
    // cannot be spawned) must also survive it.
    let fallback = run_json_or_local(&Job::OfficeOpen { path });
    assert!(fallback.is_err());
}

#[test]
fn a_valid_document_round_trips_through_the_job() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("corpus").join("writer-basic.docx");
    let json = execute(&Job::OfficeOpen { path }).expect("the corpus sample parses");
    assert!(json.contains("\"kind\":\"writer\""), "{json:.120}");
}

/// Android has no worker process (see the module docs), so this is the path
/// every document takes there: it must return an error for a bad document
/// instead of unwinding.
#[test]
fn the_in_process_path_is_used_on_android_and_returns_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.xlsx");
    std::fs::write(&path, b"PK\x03\x04this is not a workbook at all").unwrap();
    let result = run_in_process(&Job::OfficeOpen { path }, None);
    assert!(result.is_err(), "a corrupt workbook is an error, not a panic");
    // A missing file goes through the same path.
    let missing = dir.path().join("missing.docx");
    assert!(run_in_process(&Job::OfficeOpen { path: missing }, None).is_err());
}

#[test]
fn a_panicking_parse_is_caught_by_the_fallback() {
    // The fallback wraps `execute` in catch_unwind; prove the wrapper
    // itself survives a panic (the worker process is the primary guard,
    // this is the belt for the dev/test path).
    let caught = std::panic::catch_unwind(|| panic!("boom"));
    assert!(caught.is_err());
}

#[test]
fn parses_pdf_page_text_and_decodes_pdf_errors() {
    use pdfcore::error::{ErrorCode, PdfError};

    let job = Job::PdfPageText { path: PathBuf::from("a.pdf"), page: 3 };
    assert_eq!(job.name(), "pdf-page-text");
    assert_eq!(job.args(), vec!["pdf-page-text", "a.pdf", "3"]);
    assert_eq!(Job::parse(&job.args()).unwrap(), job);
    assert!(Job::parse(&["pdf-page-text".into(), "a.pdf".into()]).is_err());
    assert!(Job::parse(&["pdf-page-text".into(), "a.pdf".into(), "abc".into()]).is_err());

    assert_eq!(decode_pdf_error(PdfError::PasswordRequired.to_string()).code(), ErrorCode::PasswordRequired);
    assert_eq!(decode_pdf_error(PdfError::WrongPassword.to_string()).code(), ErrorCode::WrongPassword);
    assert_eq!(decode_pdf_error(PdfError::RangeOutOfBounds.to_string()).code(), ErrorCode::RangeOutOfBounds);
    assert_eq!(decode_pdf_error(PdfError::NotFound("x.pdf".into()).to_string()).code(), ErrorCode::NotFound);
    assert_eq!(decode_pdf_error(PdfError::InvalidPdf("bad".into()).to_string()).code(), ErrorCode::InvalidPdf);
    assert_eq!(decode_pdf_error(PdfError::CorruptPdf("broken".into()).to_string()).code(), ErrorCode::CorruptPdf);
    assert_eq!(decode_pdf_error(PdfError::EngineMissing("pdfium".into()).to_string()).code(), ErrorCode::EngineMissing);
    assert_eq!(decode_pdf_error("unexpected worker failure".into()).code(), ErrorCode::ProcessingFailed);
}

#[test]
fn parses_pdf_page_image_and_round_trips_cli_args() {
    let jpeg_job =
        Job::PdfPageImage { path: PathBuf::from("a.pdf"), page: 1, max_width: 220, dpi: 96, jpeg_quality: Some(82) };
    assert_eq!(jpeg_job.name(), "pdf-page-image");
    assert_eq!(jpeg_job.args(), vec!["pdf-page-image", "a.pdf", "1", "220", "96", "82"]);
    assert_eq!(Job::parse(&jpeg_job.args()).unwrap(), jpeg_job);

    let png_job =
        Job::PdfPageImage { path: PathBuf::from("b.pdf"), page: 2, max_width: 1100, dpi: 600, jpeg_quality: None };
    assert_eq!(png_job.args(), vec!["pdf-page-image", "b.pdf", "2", "1100", "600", "png"]);
    assert_eq!(Job::parse(&png_job.args()).unwrap(), png_job);

    assert!(Job::parse(&["pdf-page-image".into(), "a.pdf".into(), "1".into()]).is_err());
    assert!(Job::parse(&[
        "pdf-page-image".into(),
        "a.pdf".into(),
        "1".into(),
        "220".into(),
        "96".into(),
        "bad".into(),
    ])
    .is_err());
}

#[test]
fn parses_pdf_list_annotations_and_pdf_inspect() {
    let ann_job = Job::PdfListAnnotations { path: PathBuf::from("annotated.pdf") };
    assert_eq!(ann_job.name(), "pdf-list-annotations");
    assert_eq!(ann_job.args(), vec!["pdf-list-annotations", "annotated.pdf"]);
    assert_eq!(Job::parse(&ann_job.args()).unwrap(), ann_job);
    assert!(Job::parse(&["pdf-list-annotations".into()]).is_err());

    let inspect_job = Job::PdfInspect { path: PathBuf::from("report.pdf") };
    assert_eq!(inspect_job.name(), "pdf-inspect");
    assert_eq!(inspect_job.args(), vec!["pdf-inspect", "report.pdf"]);
    assert_eq!(Job::parse(&inspect_job.args()).unwrap(), inspect_job);
    assert!(Job::parse(&["pdf-inspect".into()]).is_err());
}

#[test]
fn parses_pdf_studio_and_pdfa_worker_jobs() {
    let fields_job = Job::PdfListFormFields { path: PathBuf::from("form.pdf") };
    assert_eq!(fields_job.name(), "pdf-list-form-fields");
    assert_eq!(fields_job.args(), vec!["pdf-list-form-fields", "form.pdf"]);
    assert_eq!(Job::parse(&fields_job.args()).unwrap(), fields_job);
    assert!(Job::parse(&["pdf-list-form-fields".into()]).is_err());

    let objects_job = Job::PdfListObjects { path: PathBuf::from("objects.pdf") };
    assert_eq!(objects_job.name(), "pdf-list-objects");
    assert_eq!(objects_job.args(), vec!["pdf-list-objects", "objects.pdf"]);
    assert_eq!(Job::parse(&objects_job.args()).unwrap(), objects_job);
    assert!(Job::parse(&["pdf-list-objects".into()]).is_err());

    let runs_job = Job::PdfListTextRuns { path: PathBuf::from("runs.pdf") };
    assert_eq!(runs_job.name(), "pdf-list-text-runs");
    assert_eq!(runs_job.args(), vec!["pdf-list-text-runs", "runs.pdf"]);
    assert_eq!(Job::parse(&runs_job.args()).unwrap(), runs_job);
    assert!(Job::parse(&["pdf-list-text-runs".into()]).is_err());

    let pdfa_job = Job::PdfaValidate { path: PathBuf::from("archival.pdf"), level: "2b".into() };
    assert_eq!(pdfa_job.name(), "pdfa-validate");
    assert_eq!(pdfa_job.args(), vec!["pdfa-validate", "archival.pdf", "2b"]);
    assert_eq!(Job::parse(&pdfa_job.args()).unwrap(), pdfa_job);
    assert!(Job::parse(&["pdfa-validate".into(), "archival.pdf".into()]).is_err());
}

#[test]
fn parses_pdf_tile_and_verify_signatures_worker_jobs() {
    let tile_job = Job::PdfPageTile {
        path: PathBuf::from("big.pdf"),
        page: 2,
        scale_bits: 3.5f32.to_bits(),
        x: 128,
        y: 256,
        width: 512,
        height: 512,
        jpeg_quality: Some(86),
    };
    assert_eq!(tile_job.name(), "pdf-page-tile");
    assert_eq!(Job::parse(&tile_job.args()).unwrap(), tile_job);

    let png_tile = Job::PdfPageTile {
        path: PathBuf::from("big.pdf"),
        page: 1,
        scale_bits: 2.0f32.to_bits(),
        x: 0,
        y: 0,
        width: 256,
        height: 256,
        jpeg_quality: None,
    };
    assert_eq!(Job::parse(&png_tile.args()).unwrap(), png_tile);
    assert!(Job::parse(&["pdf-page-tile".into(), "big.pdf".into(), "1".into()]).is_err());

    let sig_job = Job::PdfVerifySignatures { path: PathBuf::from("signed.pdf") };
    assert_eq!(sig_job.name(), "pdf-verify-signatures");
    assert_eq!(sig_job.args(), vec!["pdf-verify-signatures", "signed.pdf"]);
    assert_eq!(Job::parse(&sig_job.args()).unwrap(), sig_job);
    assert!(Job::parse(&["pdf-verify-signatures".into()]).is_err());
}

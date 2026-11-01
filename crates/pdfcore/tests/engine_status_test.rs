//! The engine probe's fallback (E2.3): when the blocking probe cannot run, the
//! UI must receive an "everything missing" status instead of a blank panel.

use pdfcore::engines::EngineStatus;

#[test]
fn a_failed_probe_reports_every_engine_missing() {
    let missing = EngineStatus::missing();
    assert!(!missing.pdfium && !missing.qpdf && !missing.tesseract);
    assert!(missing.tesseract_version.is_none(), "no version without a binary");
    assert!(missing.ocr_languages.is_empty(), "no languages without tessdata");
    assert!(missing.pdfium_path.is_none() && missing.tesseract_path.is_none());
}

//! Office export straight to PDF/A (#341 C5, the official letter): the
//! document is laid out to a temporary PDF on the Rust side and converted into
//! the chosen target, so the webview only ever passes the target's token.

use std::path::Path;

use pdfcore::pdfa::{PdfaLevel, PdfaReport};
use pdfcore::{CancelToken, PdfError};
use serde_json::Value;

/// Lays `model` out to a temporary PDF and converts it into `target`.
fn export_pdfa(kind: &str, model: Value, target: &Path, level: PdfaLevel) -> Result<PdfaReport, PdfError> {
    let work = pdfcore::docutil::JobDir::new("office-pdfa")?;
    let draft = work.file("export.pdf");
    crate::office::export_pdf(kind, model, &draft).map_err(|error| PdfError::ConversionFailed(error.message))?;
    let report = pdfcore::pdfa::convert_pdfa(&draft, target, level, &|_| {}, &CancelToken::new())?;
    work.cleanup();
    Ok(report)
}

/// Exports `model` as PDF and converts it to PDF/A at `level` ("1b", "2b",
/// ...) into `path`. The report says whether the result validates.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn office_export_pdfa(
    kind: String,
    model: Value,
    path: String,
    level: String,
) -> Result<PdfaReport, PdfError> {
    let level = PdfaLevel::parse(&level)?;
    let target = crate::paths::output_file(&path)?.into_path_buf();
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || export_pdfa(&kind, model, &target, level))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_writer_document_comes_out_as_valid_pdfa_2b() {
        let mut document = officecore::model::TextDocument::new_blank("Resmî yazı");
        document.blocks = vec![
            officecore::model::Block::paragraph("T.C."),
            officecore::model::Block::paragraph("Sayı : E-12345678-010.01-1 29.10.2026"),
            officecore::model::Block::paragraph("Konu : Eğitim semineri; şu, ğ, ı ve İ harfleriyle."),
        ];
        let model = serde_json::to_value(&document).expect("the model serialises");
        let dir = tempfile::tempdir().expect("a temp dir");
        let target = dir.path().join("letter.pdf");

        let report = export_pdfa("writer", model, &target, PdfaLevel::parse("2b").unwrap()).expect("the export runs");
        let failing: Vec<_> = report.checks.iter().filter(|check| check.status == "fail").collect();
        assert!(report.valid, "PDF/A-2b checks failing: {failing:?}");
        assert!(report.converted);
        assert!(std::fs::metadata(&target).map(|meta| meta.len() > 1000).unwrap_or(false));
    }

    #[test]
    fn an_unknown_kind_is_refused_without_writing_the_target() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let target = dir.path().join("out.pdf");
        let result = export_pdfa("drawing", Value::Null, &target, PdfaLevel::parse("2b").unwrap());
        assert!(matches!(result, Err(PdfError::ConversionFailed(_))));
        assert!(!target.exists());
    }
}

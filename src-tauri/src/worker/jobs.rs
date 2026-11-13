//! Worker job execution and cross-process error decoding.
//!
//! Each parser job serializes its output as JSON on stdout; errors cross the
//! process boundary as their `Display` strings on stderr and are reconstructed
//! into [`PdfError`] variants by [`decode_pdf_error`].

use super::Job;
use pdfcore::error::PdfError;

/// Runs `job` and returns its JSON result. This is what the worker process
/// executes; [`super::run_json_or_local`] also uses it as the in-process
/// fallback.
pub fn execute(job: &Job) -> Result<String, String> {
    match job {
        Job::OfficeOpen { path } => {
            let document = crate::office::open_path(path).map_err(|error| error.to_string())?;
            to_json(&document)
        }
        Job::InvoiceOpen { path } => {
            let document = crate::office::invoice_document(path).map_err(|error| error.to_string())?;
            to_json(&document)
        }
        Job::InvoicePackage { path } => {
            let documents = crate::office::invoice_package(path).map_err(|error| error.to_string())?;
            to_json(&documents)
        }
        Job::EmlOpen { path } => {
            let message = crate::office::eml_message(path).map_err(|error| error.to_string())?;
            to_json(&message)
        }
        Job::PdfInfo { path } => {
            let info = pdfcore::info::pdf_info(path, None).map_err(|error| error.to_string())?;
            to_json(&info)
        }
        Job::PdfPageText { path, page } => {
            let text = pdfcore::render::extract_page_text(path, None, *page).map_err(|error| error.to_string())?;
            to_json(&text)
        }
        Job::Repair { input, output } => {
            let cancel = pdfcore::progress::CancelToken::new();
            let silent = |_| ();
            let report =
                pdfcore::repair::repair_pdf(input, output, &silent, &cancel).map_err(|error| error.to_string())?;
            to_json(&report)
        }
    }
}

/// Reconstructs a [`PdfError`] from the worker's stderr message so UI error
/// codes match the in-process call path.
pub fn decode_pdf_error(msg: String) -> PdfError {
    if let Some(rest) = msg.strip_prefix("invalid PDF: ") {
        PdfError::InvalidPdf(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("corrupt PDF: ") {
        PdfError::CorruptPdf(rest.to_string())
    } else if msg == "document is password protected" {
        PdfError::PasswordRequired
    } else if msg == "incorrect password" {
        PdfError::WrongPassword
    } else if let Some(rest) = msg.strip_prefix("file not found: ") {
        PdfError::NotFound(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("permission denied: ") {
        PdfError::PermissionDenied(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("file is in use by another program: ") {
        PdfError::FileLocked(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("output file already exists: ") {
        PdfError::OutputExists(rest.to_string())
    } else if msg == "not enough disk space" {
        PdfError::DiskFull
    } else if msg == "operation cancelled" {
        PdfError::Cancelled
    } else if let Some(rest) = msg.strip_prefix("required engine is not available: ") {
        PdfError::EngineMissing(rest.to_string())
    } else if msg == "OCR engine is not available" {
        PdfError::OcrEngineUnavailable
    } else if let Some(rest) = msg.strip_prefix("unsupported input: ") {
        PdfError::Unsupported(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("invalid input: ") {
        PdfError::InvalidInput(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("invalid image: ") {
        PdfError::InvalidImage(rest.to_string())
    } else if msg == "page selection is out of bounds" {
        PdfError::RangeOutOfBounds
    } else if let Some(rest) = msg.strip_prefix("conversion failed: ") {
        PdfError::ConversionFailed(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("processing failed: ") {
        PdfError::ProcessingFailed(rest.to_string())
    } else if let Some(rest) = msg.strip_prefix("internal error: ") {
        PdfError::Internal(rest.to_string())
    } else {
        PdfError::ProcessingFailed(msg)
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

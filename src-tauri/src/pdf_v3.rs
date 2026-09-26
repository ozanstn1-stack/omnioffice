//! V3.0 PDF commands: sanitizer, PDF/A validation and conversion, and
//! annotation/form flattening. All long operations run through the shared job
//! registry so the UI gets progress and cancellation for free.

use crate::commands::{operation_with_progress, OutputSpec};
use crate::jobs::JobRegistry;
use pdfcore::error::PdfError;
use pdfcore::flatten::FlattenReport;
use pdfcore::pdfa::{PdfaLevel, PdfaReport};
use pdfcore::sanitize::SanitizeReport;
use std::path::Path;
use tauri::{AppHandle, State};

fn level_from(value: &str) -> Result<PdfaLevel, PdfError> {
    PdfaLevel::parse(value)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizeRequest {
    pub input: String,
    pub output: Option<OutputSpec>,
    #[serde(default)]
    pub options: Option<pdfcore::sanitize::SanitizeOptions>,
    #[serde(default)]
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn sanitize_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: SanitizeRequest,
) -> Result<SanitizeReport, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(Path::new(&request.input), "-clean"),
        };
        let options = request.options.clone().unwrap_or_default();
        pdfcore::sanitize::sanitize_pdf(Path::new(&request.input), &output, &options, progress, cancel)
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenRequest {
    pub input: String,
    pub output: Option<OutputSpec>,
    #[serde(default)]
    pub options: Option<pdfcore::flatten::FlattenOptions>,
    #[serde(default)]
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn flatten_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: FlattenRequest,
) -> Result<FlattenReport, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(Path::new(&request.input), "-flat"),
        };
        let options = request.options.clone().unwrap_or_default();
        pdfcore::flatten::flatten_pdf(Path::new(&request.input), &output, &options, progress, cancel)
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfaRequest {
    pub input: String,
    pub level: String,
    #[serde(default)]
    pub output: Option<OutputSpec>,
    #[serde(default)]
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn pdfa_validate(request: PdfaRequest) -> Result<PdfaReport, PdfError> {
    let level = level_from(&request.level)?;
    let input = request.input.clone();
    tauri::async_runtime::spawn_blocking(move || pdfcore::pdfa::validate_pdfa(Path::new(&input), level))
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[tauri::command]
pub async fn pdfa_convert(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PdfaRequest,
) -> Result<PdfaReport, PdfError> {
    let level = level_from(&request.level)?;
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(Path::new(&request.input), "-pdfa"),
        };
        pdfcore::pdfa::convert_pdfa(Path::new(&request.input), &output, level, progress, cancel)
    })
    .await
}

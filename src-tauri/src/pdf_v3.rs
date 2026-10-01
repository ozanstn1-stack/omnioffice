//! V3.0 PDF commands: sanitizer, PDF/A validation and conversion, and
//! annotation/form flattening. All long operations run through the shared job
//! registry so the UI gets progress and cancellation for free.
//!
//! V3.1 adds the AcroForm field commands (list/fill/validate) and the PDF
//! Studio object commands (list/edit). Those are short local file operations,
//! so they run directly on the blocking pool; nothing here fakes form state -
//! every result comes from `pdfcore::forms` reading and writing real PDF
//! objects.

use crate::commands::{operation_with_progress, OutputSpec};
use crate::jobs::JobRegistry;
use pdfcore::error::PdfError;
use pdfcore::flatten::FlattenReport;
use pdfcore::pdfa::{PdfaLevel, PdfaReport};
use pdfcore::sanitize::SanitizeReport;
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
        let input = crate::paths::input_file(&request.input)?;
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(input.as_path(), "-clean"),
        };
        let options = request.options.clone().unwrap_or_default();
        pdfcore::sanitize::sanitize_pdf(input.as_path(), &output, &options, progress, cancel)
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
        let input = crate::paths::input_file(&request.input)?;
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(input.as_path(), "-flat"),
        };
        let options = request.options.clone().unwrap_or_default();
        pdfcore::flatten::flatten_pdf(input.as_path(), &output, &options, progress, cancel)
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
    let input = crate::paths::input_file(&request.input)?.into_path_buf();
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || pdfcore::pdfa::validate_pdfa(&input, level))
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
        let input = crate::paths::input_file(&request.input)?;
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(input.as_path(), "-pdfa"),
        };
        pdfcore::pdfa::convert_pdfa(input.as_path(), &output, level, progress, cancel)
    })
    .await
}

// ---------------------------------------------------------------------------
// V3.1: AcroForm fields
// ---------------------------------------------------------------------------

/// Lists every terminal form field with its type, flags, value, options,
/// widget page/rect and tab order. Password protected inputs report
/// `PasswordRequired` instead of silently returning an empty list.
#[tauri::command]
pub async fn pdf_list_form_fields(
    path: String,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::FormFieldInfo>, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        pdfcore::forms::list_fields_in_file(path.as_path(), password.as_deref())
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillFormRequest {
    pub input: String,
    pub output: OutputSpec,
    #[serde(default)]
    pub values: Vec<pdfcore::forms::FieldValue>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Writes the requested values into the AcroForm, regenerates the widget
/// appearances and saves to `output`. Unknown field names fail atomically
/// before anything is written.
#[tauri::command]
pub async fn pdf_fill_form(request: FillFormRequest) -> Result<pdfcore::forms::FillReport, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let mut document = pdfcore::docutil::load_document(input.as_path(), request.password.as_deref())?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let report = pdfcore::forms::apply_field_values(&mut document, &request.values)?;
        pdfcore::docutil::save_document(&mut document, &target, true)?;
        Ok(report)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

/// Checks the given values against the form. Only provable constraints are
/// reported; PDF JavaScript is never executed and scripted formats are
/// flagged as unchecked warnings instead.
#[tauri::command]
pub async fn pdf_validate_form(
    path: String,
    values: Vec<pdfcore::forms::FieldValue>,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::FieldIssue>, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        let document = pdfcore::docutil::load_document(path.as_path(), password.as_deref())?;
        Ok(pdfcore::forms::validate_fields(&document, &values))
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

// ---------------------------------------------------------------------------
// V3.1: PDF Studio page objects
// ---------------------------------------------------------------------------

/// Lists annotations, widgets and drawn image placements for every page.
#[tauri::command]
pub async fn pdf_list_objects(
    path: String,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::PageObjectInfo>, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        pdfcore::forms::list_page_objects_in_file(path.as_path(), password.as_deref())
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditObjectsRequest {
    pub input: String,
    pub output: OutputSpec,
    #[serde(default)]
    pub edits: Vec<pdfcore::forms::ObjectEdit>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Applies annotation/widget/image edits and saves to `output`. Edits are
/// validated against a snapshot of the original object list before any
/// mutation, so a bad index cannot leave a half-edited file.
#[tauri::command]
pub async fn pdf_edit_objects(request: EditObjectsRequest) -> Result<pdfcore::forms::EditReport, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let mut document = pdfcore::docutil::load_document(input.as_path(), request.password.as_deref())?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let report = pdfcore::forms::apply_object_edits(&mut document, &request.edits)?;
        pdfcore::docutil::save_document(&mut document, &target, true)?;
        Ok(report)
    })
    .await
    .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

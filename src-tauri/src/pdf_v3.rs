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
use crate::worker::{run_json_decoded, Job};
use pdfcore::error::PdfError;
use pdfcore::flatten::FlattenReport;
use pdfcore::pdfa::{PdfaLevel, PdfaReport};
use pdfcore::sanitize::SanitizeReport;
use tauri::{AppHandle, State};

#[cfg_attr(feature = "bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "bindings", specta::specta)]
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

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairRequest {
    pub input: String,
    pub output: Option<OutputSpec>,
    #[serde(default)]
    pub job_id: Option<String>,
}

/// Repairs a damaged PDF: qpdf when it is available, otherwise (Android, or
/// when qpdf fails) the built-in rebuild. The report names the method used.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_repair(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: RepairRequest,
) -> Result<pdfcore::repair::RepairReport, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let input = crate::paths::input_file(&request.input)?;
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(input.as_path(), "-repaired"),
        };
        // Repair walks a possibly damaged file with lopdf/qpdf; it runs in a
        // worker process so a crash cannot take the app down (see
        // `crate::worker`). Progress stays coarse: the worker is one shot.
        progress(pdfcore::progress::ProgressEvent::new("repair", 0, 1));
        let json = crate::worker::run_json_or_local(&crate::worker::Job::Repair {
            input: input.as_path().to_path_buf(),
            output,
        })
        .map_err(PdfError::ProcessingFailed)?;
        cancel.check()?;
        progress(pdfcore::progress::ProgressEvent::new("repair", 1, 1));
        serde_json::from_str(&json)
            .map_err(|error| PdfError::Internal(format!("the repair result is not valid: {error}")))
    })
    .await
}

/// Rewrites a PDF in qpdf's linearized (Fast Web View) layout.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_linearize(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: RepairRequest,
) -> Result<pdfcore::repair::RepairReport, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let input = crate::paths::input_file(&request.input)?;
        let output = match &request.output {
            Some(spec) => spec.resolve()?.0,
            None => pdfcore::docutil::default_output_for(input.as_path(), "-linearized"),
        };
        pdfcore::repair::linearize_pdf(input.as_path(), &output, progress, cancel)
    })
    .await
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "bindings", specta::specta)]
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

#[cfg_attr(feature = "bindings", derive(specta::Type))]
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

async fn run_blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, PdfError> + Send + 'static,
) -> Result<T, PdfError> {
    let _permit = crate::concurrency::acquire().await;
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|error| PdfError::Internal(format!("worker thread failed: {error}")))?
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdfa_validate(request: PdfaRequest) -> Result<PdfaReport, PdfError> {
    let _level = PdfaLevel::parse(&request.level)?;
    let input = crate::paths::input_file(&request.input)?.into_path_buf();
    let level = request.level;
    run_blocking(move || run_json_decoded(&Job::PdfaValidate { path: input, level })).await
}

#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdfa_convert(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PdfaRequest,
) -> Result<PdfaReport, PdfError> {
    let level = PdfaLevel::parse(&request.level)?;
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
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_list_form_fields(
    path: String,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::FormFieldInfo>, PdfError> {
    run_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        if password.is_none() {
            return run_json_decoded(&Job::PdfListFormFields { path: path.into_path_buf() });
        }
        pdfcore::forms::list_fields_in_file(path.as_path(), password.as_deref())
    })
    .await
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_fill_form(request: FillFormRequest) -> Result<pdfcore::forms::FillReport, PdfError> {
    run_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let mut document = pdfcore::docutil::load_document(input.as_path(), request.password.as_deref())?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let report = pdfcore::forms::apply_field_values(&mut document, &request.values)?;
        pdfcore::docutil::save_document(&mut document, &target, true)?;
        Ok(report)
    })
    .await
}

/// Checks the given values against the form. Only provable constraints are
/// reported; PDF JavaScript is never executed and scripted formats are
/// flagged as unchecked warnings instead.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_validate_form(
    path: String,
    values: Vec<pdfcore::forms::FieldValue>,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::FieldIssue>, PdfError> {
    run_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        let document = pdfcore::docutil::load_document(path.as_path(), password.as_deref())?;
        Ok(pdfcore::forms::validate_fields(&document, &values))
    })
    .await
}

// ---------------------------------------------------------------------------
// V3.1: PDF Studio page objects
// ---------------------------------------------------------------------------

/// Lists annotations, widgets and drawn image placements for every page.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_list_objects(
    path: String,
    password: Option<String>,
) -> Result<Vec<pdfcore::forms::PageObjectInfo>, PdfError> {
    run_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        if password.is_none() {
            return run_json_decoded(&Job::PdfListObjects { path: path.into_path_buf() });
        }
        pdfcore::forms::list_page_objects_in_file(path.as_path(), password.as_deref())
    })
    .await
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_edit_objects(request: EditObjectsRequest) -> Result<pdfcore::forms::EditReport, PdfError> {
    run_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let mut document = pdfcore::docutil::load_document(input.as_path(), request.password.as_deref())?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let report = pdfcore::forms::apply_object_edits(&mut document, &request.edits)?;
        pdfcore::docutil::save_document(&mut document, &target, true)?;
        Ok(report)
    })
    .await
}

// ---------------------------------------------------------------------------
// V3.6: content-stream text runs
// ---------------------------------------------------------------------------

/// Lists the text runs of every page (stream order, approximate positions).
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_list_text_runs(
    path: String,
    password: Option<String>,
) -> Result<Vec<pdfcore::content::TextRunInfo>, PdfError> {
    run_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        if password.is_none() {
            return run_json_decoded(&Job::PdfListTextRuns { path: path.into_path_buf() });
        }
        pdfcore::content::list_text_runs_in_file(path.as_path(), password.as_deref())
    })
    .await
}

#[cfg_attr(feature = "bindings", derive(specta::Type))]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTextRunsRequest {
    pub input: String,
    pub output: OutputSpec,
    #[serde(default)]
    pub edits: Vec<pdfcore::content::TextRunEdit>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Replaces the text of existing runs and appends the result as a new
/// revision: the original bytes (and any signature over them) stay intact.
#[tauri::command]
#[cfg_attr(feature = "bindings", specta::specta)]
pub async fn pdf_edit_text_runs(request: EditTextRunsRequest) -> Result<pdfcore::content::TextEditReport, PdfError> {
    run_blocking(move || {
        let input = crate::paths::input_file(&request.input)?;
        let (output, policy) = request.output.resolve()?;
        let target = pdfcore::docutil::resolve_output_path(&output, policy)?;
        let original = std::fs::read(input.as_path()).map_err(PdfError::from_io)?;
        // Encrypted inputs are refused by the content editor; the password
        // would have to be applied before the bytes are re-encoded.
        if pdfcore::docutil::looks_encrypted(input.as_path()) {
            return Err(PdfError::PasswordRequired);
        }
        let _ = request.password;
        let (edited, report) = pdfcore::content::edit_text_runs(&original, &request.edits)?;
        pdfcore::docutil::write_bytes_atomic(&target, &edited)?;
        Ok(report)
    })
    .await
}

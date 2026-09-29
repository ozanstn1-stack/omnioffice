//! Tauri command surface. Each command maps a UI request to one pdfcore
//! operation, forwarding progress events and honoring cancellation.
//!
//! Privacy notes:
//! * Passwords are accepted as parameters but never logged, never persisted.
//! * Only file paths, timestamps and tool names are stored for "recent files".
//! * Everything runs locally in-process (plus bundled pdfium/tesseract/qpdf).

use crate::jobs::{emit_progress, JobRegistry};
use base64::Engine;
use pdfcore::docutil::OverwritePolicy;
use pdfcore::engines::{self, EngineStatus, OcrLanguage};
use pdfcore::error::PdfError;
use pdfcore::pages::SplitMode;
use pdfcore::progress::CancelToken;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

fn policy(value: &Option<String>) -> OverwritePolicy {
    match value.as_deref() {
        Some("replace") => OverwritePolicy::Replace,
        Some("unique_name") => OverwritePolicy::UniqueName,
        _ => OverwritePolicy::Error,
    }
}

async fn run_blocking<T, F>(work: F) -> Result<T, PdfError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, PdfError> + Send + 'static,
{
    // Commands are async; heavy work happens on the blocking pool so the UI
    // thread and the async runtime stay responsive. Await the handle instead
    // of block_on(): blocking inside an async runtime would panic.
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| PdfError::Internal(format!("worker thread failed: {e}")))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpResult {
    pub path: String,
    pub page_count: Option<u32>,
    pub original_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    pub reduction: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpec {
    pub path: String,
    #[serde(default)]
    pub overwrite: Option<String>,
}

impl OutputSpec {
    pub(crate) fn resolve(&self) -> Result<(PathBuf, OverwritePolicy), PdfError> {
        let path = PathBuf::from(&self.path);
        if path.as_os_str().is_empty() {
            return Err(PdfError::InvalidInput("output path is empty".into()));
        }
        Ok((path, policy(&self.overwrite)))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnail {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_version: String,
    pub core_version: String,
    pub platform: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecentEntry {
    pub path: String,
    pub file_name: String,
    pub tool: String,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// System commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        core_version: pdfcore::VERSION.to_string(),
        platform: std::env::consts::OS.to_string(),
        name: "PDF Swiss Army Knife".to_string(),
    }
}

#[tauri::command]
pub fn engine_status() -> EngineStatus {
    engines::engine_status()
}

#[tauri::command]
pub fn ocr_languages() -> Vec<OcrLanguage> {
    engines::ocr_languages()
}

#[tauri::command]
pub fn cancel_job(registry: State<'_, JobRegistry>, job_id: String) {
    registry.cancel(&job_id);
}

// ---------------------------------------------------------------------------
// Inspection
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn pdf_info(path: String, password: Option<String>) -> Result<pdfcore::info::PdfInfo, PdfError> {
    run_blocking(move || pdfcore::info::pdf_info(Path::new(&path), password.as_deref())).await
}

#[tauri::command]
pub async fn page_thumbnail(
    path: String,
    page: u32,
    max_width: Option<u32>,
    password: Option<String>,
) -> Result<Thumbnail, PdfError> {
    run_blocking(move || {
        let max_width = max_width.unwrap_or(220).clamp(60, 3000);
        let options = pdfcore::render::RenderOptions {
            dpi: 96.0,
            max_width: Some(max_width),
            max_height: None,
        };
        let rendered = pdfcore::render::render_page(Path::new(&path), password.as_deref(), page, &options)?;
        let bytes = pdfcore::images::encode_image(
            &rendered,
            pdfcore::images::ImageFormat::Jpeg,
            82,
            false,
        )?;
        let data_url = format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        Ok(Thumbnail {
            data_url,
            width: rendered.width,
            height: rendered.height,
        })
    })
    .await
}

#[tauri::command]
pub async fn page_preview(
    path: String,
    page: u32,
    max_width: Option<u32>,
    password: Option<String>,
    format: Option<String>,
    quality: Option<u8>,
) -> Result<Thumbnail, PdfError> {
    run_blocking(move || {
        let max_width = max_width.unwrap_or(1100).clamp(200, 4000);
        let options = pdfcore::render::RenderOptions {
            dpi: 96.0,
            max_width: Some(max_width),
            max_height: None,
        };
        let rendered = pdfcore::render::render_page(Path::new(&path), password.as_deref(), page, &options)?;
        // Reading mode requests JPEG (much smaller for large pages); the
        // thumbnail/preview default stays lossless PNG.
        let (bytes, mime) = match format.as_deref() {
            Some("jpeg") | Some("jpg") => (
                pdfcore::images::encode_image(
                    &rendered,
                    pdfcore::images::ImageFormat::Jpeg,
                    quality.unwrap_or(86),
                    false,
                )?,
                "image/jpeg",
            ),
            _ => (
                pdfcore::images::encode_image(&rendered, pdfcore::images::ImageFormat::Png, 90, false)?,
                "image/png",
            ),
        };
        let data_url = format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        Ok(Thumbnail {
            data_url,
            width: rendered.width,
            height: rendered.height,
        })
    })
    .await
}

/// Extracts the text layer of a single page (reading mode: "copy page text").
#[tauri::command]
pub async fn page_text(path: String, page: u32, password: Option<String>) -> Result<String, PdfError> {
    run_blocking(move || pdfcore::render::extract_page_text(Path::new(&path), password.as_deref(), page)).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub matches: Vec<pdfcore::render::TextMatch>,
    pub pages_with_matches: u32,
    pub total_matches: u32,
    pub truncated: bool,
}

/// Full-text search over the document's text layer (reading mode).
#[tauri::command]
pub async fn search_document(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    path: String,
    query: String,
    match_case: Option<bool>,
    max_results: Option<u32>,
    password: Option<String>,
    job_id: Option<String>,
) -> Result<SearchResponse, PdfError> {
    operation_with_progress(app, registry, job_id, move |progress, cancel| {
        let result = pdfcore::render::search_document(
            Path::new(&path),
            password.as_deref(),
            &query,
            match_case.unwrap_or(false),
            max_results.unwrap_or(200).clamp(1, 2000),
            cancel,
            &|current, total| {
                progress(pdfcore::progress::ProgressEvent::new("search.page", current as u64, total as u64));
            },
        )?;
        Ok(SearchResponse {
            matches: result.matches,
            pages_with_matches: result.pages_with_matches,
            total_matches: result.total_matches,
            truncated: result.truncated,
        })
    })
    .await
}

#[tauri::command]
pub async fn check_password(path: String, password: String) -> Result<bool, PdfError> {
    run_blocking(move || pdfcore::security::check_password(Path::new(&path), &password)).await
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub inputs: Vec<String>,
    pub output: OutputSpec,
    #[serde(default = "default_true")]
    pub preserve_metadata: bool,
    pub job_id: Option<String>,
}

fn default_true() -> bool {
    true
}

#[tauri::command]
pub async fn merge_pdfs(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: MergeRequest,
) -> Result<OpResult, PdfError> {
    let job_id = request.job_id.clone().unwrap_or_else(|| "merge".into());
    let cancel = registry.register(&job_id);
    let app_for_progress = app.clone();
    let job_for_progress = job_id.clone();
    let result = run_blocking(move || {
        let progress = move |event: pdfcore::progress::ProgressEvent| {
            emit_progress(&app_for_progress, &job_for_progress, &event);
        };
        let (output, policy) = request.output.resolve()?;
        let inputs: Vec<PathBuf> = request.inputs.iter().map(PathBuf::from).collect();
        if inputs.len() < 2 {
            return Err(PdfError::InvalidInput("select at least two PDF files".into()));
        }
        let (path, pages) = pdfcore::merge::merge_files(
            &inputs,
            &output,
            &pdfcore::merge::MergeOptions {
                preserve_metadata: request.preserve_metadata,
            },
            policy,
            &progress,
            &cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(pages),
            original_bytes: inputs.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).reduce(|a, b| a + b),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await;
    registry.finish(&job_id);
    result
}

// ---------------------------------------------------------------------------
// Page operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagesRequest {
    pub input: String,
    #[serde(default)]
    pub pages: Vec<u32>,
    pub output: OutputSpec,
    #[serde(default)]
    pub selection: Option<String>,
    #[serde(default)]
    pub degrees: Option<i32>,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn extract_pages(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PagesRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        validate_pages(Path::new(&request.input), &request)?;
        emit_progress_simple(progress, "extract", 0, 1);
        let path = pdfcore::organize::extract_pages(
            Path::new(&request.input),
            &request.pages,
            &output,
            policy,
            request.password.as_deref(),
        )?;
        cancel.check()?;
        let pages = pdfcore::info::pdf_info(&path, None).map(|i| i.page_count).unwrap_or(0);
        emit_progress_simple(progress, "extract", 1, 1);
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(pages),
            original_bytes: std::fs::metadata(&request.input).ok().map(|m| m.len()),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

fn validate_pages(
    input: &Path,
    request: &PagesRequest,
) -> Result<Vec<u32>, PdfError> {
    if request.pages.is_empty() {
        if let Some(selection) = &request.selection {
            let doc = pdfcore::docutil::load_document(input, request.password.as_deref())?;
            let total = doc.get_pages().len() as u32;
            return pdfcore::pages::parse_page_selection(selection, total);
        }
        return Err(PdfError::InvalidInput("no pages selected".into()));
    }
    Ok(request.pages.clone())
}

#[tauri::command]
pub async fn delete_pages(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PagesRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let pages = validate_pages(Path::new(&request.input), &request)?;
        let path = pdfcore::organize::delete_pages(
            Path::new(&request.input),
            &pages,
            &output,
            policy,
            request.password.as_deref(),
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[tauri::command]
pub async fn rotate_pages(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PagesRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let pages = validate_pages(Path::new(&request.input), &request)?;
        let degrees = request.degrees.unwrap_or(90);
        let path = pdfcore::organize::rotate_pages(
            Path::new(&request.input),
            &pages,
            degrees,
            &output,
            policy,
            request.password.as_deref(),
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub input: String,
    pub plan: Vec<pdfcore::docutil::PagePlanItem>,
    pub output: OutputSpec,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

/// Organizer "Apply": order, deletions, duplicates and rotations in one step.
#[tauri::command]
pub async fn apply_page_plan(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PlanRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        emit_progress_simple(progress, "organize", 0, 1);
        let path = pdfcore::organize::apply_page_plan(
            Path::new(&request.input),
            &request.plan,
            &output,
            policy,
            request.password.as_deref(),
        )?;
        cancel.check()?;
        emit_progress_simple(progress, "organize", 1, 1);
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(request.plan.len() as u32),
            original_bytes: std::fs::metadata(&request.input).ok().map(|m| m.len()),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitRequest {
    pub input: String,
    pub mode: SplitMode,
    pub output_dir: String,
    #[serde(default)]
    pub overwrite: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitResponse {
    pub parts: Vec<pdfcore::organize::SplitPart>,
    pub output_dir: String,
}

#[tauri::command]
pub async fn split_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: SplitRequest,
) -> Result<SplitResponse, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let parts = pdfcore::organize::split_pdf(
            Path::new(&request.input),
            &request.mode,
            Path::new(&request.output_dir),
            policy(&request.overwrite),
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(SplitResponse {
            parts,
            output_dir: request.output_dir.clone(),
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressRequest {
    pub input: String,
    pub output: OutputSpec,
    pub options: pdfcore::compress::CompressOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn estimate_compression(
    input: String,
    options: pdfcore::compress::CompressOptions,
    password: Option<String>,
) -> Result<pdfcore::compress::CompressEstimate, PdfError> {
    run_blocking(move || {
        pdfcore::compress::estimate_compression(Path::new(&input), &options, password.as_deref())
    }).await
}

#[tauri::command]
pub async fn compress_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: CompressRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let result = pdfcore::compress::compress_pdf(
            Path::new(&request.input),
            &output,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: result.path.clone(),
            page_count: None,
            original_bytes: Some(result.original_bytes),
            output_bytes: Some(result.output_bytes),
            reduction: Some(result.reduction),
            message: None,
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// OCR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRequest {
    pub input: String,
    pub output: OutputSpec,
    pub options: pdfcore::ocr::OcrOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn ocr_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: OcrRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let result = pdfcore::ocr::ocr_pdf(
            Path::new(&request.input),
            &output,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: result.path.clone(),
            page_count: Some(result.pages_processed),
            original_bytes: std::fs::metadata(&request.input).ok().map(|m| m.len()),
            output_bytes: std::fs::metadata(&result.path).ok().map(|m| m.len()),
            reduction: None,
            message: Some(format!(
                "{} pages, {} characters, {} ms",
                result.pages_processed, result.characters, result.duration_ms
            )),
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectRequest {
    pub input: String,
    pub output: OutputSpec,
    pub user_password: String,
    pub owner_password: String,
    #[serde(default = "default_true")]
    pub allow_printing: bool,
    #[serde(default = "default_true")]
    pub allow_copying: bool,
    #[serde(default = "default_true")]
    pub allow_editing: bool,
    #[serde(default = "default_true")]
    pub allow_commenting: bool,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn protect_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: ProtectRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::security::protect_pdf(
            Path::new(&request.input),
            &output,
            &pdfcore::security::ProtectOptions {
                user_password: request.user_password.clone(),
                owner_password: request.owner_password.clone(),
                allow_printing: request.allow_printing,
                allow_copying: request.allow_copying,
                allow_editing: request.allow_editing,
                allow_commenting: request.allow_commenting,
            },
            policy,
            request.password.as_deref(),
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockRequest {
    pub input: String,
    pub output: OutputSpec,
    pub password: String,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn unlock_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: UnlockRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::security::unlock_pdf(
            Path::new(&request.input),
            &output,
            &request.password,
            policy,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesRequest {
    pub input: String,
    pub output_dir: String,
    pub format: pdfcore::images::ImageFormat,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    #[serde(default = "default_quality")]
    pub jpeg_quality: u8,
    #[serde(default)]
    pub grayscale: bool,
    #[serde(default = "default_prefix")]
    pub name_prefix: String,
    #[serde(default)]
    pub pages: Vec<u32>,
    #[serde(default)]
    pub overwrite: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

fn default_dpi() -> u32 {
    150
}
fn default_quality() -> u8 {
    90
}
fn default_prefix() -> String {
    "page".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfToImagesResponse {
    pub files: Vec<pdfcore::convert::ImageOutput>,
    pub total_bytes: u64,
    pub dpi: u32,
    pub format: String,
}

#[tauri::command]
pub async fn pdf_to_images(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: PdfToImagesRequest,
) -> Result<PdfToImagesResponse, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let result = pdfcore::convert::pdf_to_images(
            Path::new(&request.input),
            Path::new(&request.output_dir),
            request.format,
            request.dpi,
            request.jpeg_quality,
            request.grayscale,
            &request.name_prefix,
            &request.pages,
            policy(&request.overwrite),
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(PdfToImagesResponse {
            files: result.files,
            total_bytes: result.total_bytes,
            dpi: result.dpi,
            format: result.format,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagesToPdfRequest {
    pub items: Vec<pdfcore::images::ImageItem>,
    pub output: OutputSpec,
    pub options: pdfcore::images::ImageToPdfOptions,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn images_to_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: ImagesToPdfRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::images::images_to_pdf(
            &request.items,
            &request.options,
            &output,
            policy,
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(request.items.len() as u32),
            original_bytes: request
                .items
                .iter()
                .filter_map(|i| std::fs::metadata(&i.path).ok())
                .map(|m| m.len())
                .reduce(|a, b| a + b),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Layout / metadata / stamps
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeRequest {
    pub input: String,
    pub output: OutputSpec,
    pub options: pdfcore::pagelayout::ResizeOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn resize_pages(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: ResizeRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::pagelayout::resize_pages(
            Path::new(&request.input),
            &output,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRequest {
    pub input: String,
    pub output: OutputSpec,
    pub crops: Vec<pdfcore::pagelayout::CropItem>,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn crop_pages(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: CropRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::pagelayout::crop_pages(
            Path::new(&request.input),
            &output,
            &request.crops,
            policy,
            request.password.as_deref(),
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataRequest {
    pub input: String,
    pub output: OutputSpec,
    pub metadata: pdfcore::metadata::PdfMetadata,
    #[serde(default)]
    pub remove: bool,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn edit_metadata(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: MetadataRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |_progress, _cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::metadata::edit_metadata_file(
            Path::new(&request.input),
            &output,
            &request.metadata,
            request.remove,
            policy,
            request.password.as_deref(),
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NumberingRequest {
    pub input: String,
    pub output: OutputSpec,
    pub options: pdfcore::numbering::NumberingOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn add_page_numbers(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: NumberingRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::numbering::add_page_numbers(
            Path::new(&request.input),
            &output,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatermarkRequest {
    pub input: String,
    pub output: OutputSpec,
    pub options: pdfcore::watermark::WatermarkOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn watermark_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: WatermarkRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::watermark::add_watermark(
            Path::new(&request.input),
            &output,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotateRequest {
    pub input: String,
    pub output: OutputSpec,
    pub annotations: Vec<pdfcore::annotate::Annotation>,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn annotate_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AnnotateRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let path = pdfcore::annotate::annotate_pdf(
            Path::new(&request.input),
            &output,
            &request.annotations,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: None,
            original_bytes: None,
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactRequest {
    pub input: String,
    pub output: OutputSpec,
    pub areas: Vec<pdfcore::redact::RedactionArea>,
    pub options: pdfcore::redact::RedactionOptions,
    #[serde(default)]
    pub password: Option<String>,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn redact_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: RedactRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let result = pdfcore::redact::redact_pdf(
            Path::new(&request.input),
            &output,
            &request.areas,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        let message = if result.characters_removed == 0 && result.unmatched_areas > 0 {
            Some(format!("{} area(s) did not match any text.", result.unmatched_areas))
        } else {
            None
        };
        Ok(OpResult {
            path: result.output.clone(),
            page_count: None,
            original_bytes: None,
            output_bytes: Some(std::fs::metadata(&result.output).map(|m| m.len()).unwrap_or(0)),
            reduction: None,
            message,
        })
    })
    .await
}

/// Patterns found on one page, so the UI can show the user what it is about to
/// remove and let them deselect any of it.
#[tauri::command]
pub async fn detect_sensitive_text(
    path: String,
    page: u32,
    password: Option<String>,
) -> Result<Vec<pdfcore::redact::RedactionMatch>, PdfError> {
    run_blocking(move || {
        let cancel = CancelToken::new();
        let (pages, _) = pdfcore::textbox::page_chars(
            Path::new(&path),
            password.as_deref(),
            &[page],
            1,
            &cancel,
            &|_, _| {},
        )?;
        let chars = pages.get(&page).ok_or_else(|| {
            PdfError::Internal(format!("page {page} has no extractable text"))
        })?;
        Ok(pdfcore::redact::detect_sensitive(chars))
    })
    .await
}

// ---------------------------------------------------------------------------
// Compare
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub left: String,
    pub right: String,
    #[serde(default)]
    pub left_password: Option<String>,
    #[serde(default)]
    pub right_password: Option<String>,
    pub options: pdfcore::compare::CompareOptions,
    pub job_id: Option<String>,
}

#[tauri::command]
pub async fn compare_pdfs(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: CompareRequest,
) -> Result<pdfcore::compare::CompareReport, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        pdfcore::compare::compare_pdfs(
            Path::new(&request.left),
            Path::new(&request.right),
            request.left_password.as_deref(),
            request.right_password.as_deref(),
            &request.options,
            progress,
            cancel,
        )
    })
    .await
}

// ---------------------------------------------------------------------------
// Inspector
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn inspect_document(
    path: String,
    password: Option<String>,
) -> Result<pdfcore::inspect::DocumentInspection, PdfError> {
    run_blocking(move || pdfcore::inspect::inspect_document(Path::new(&path), password.as_deref())).await
}

// ---------------------------------------------------------------------------
// Settings / recent files (paths + timestamps only, never content)
// ---------------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| PdfError::Internal(format!("config dir unavailable: {e}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

/// Reads a text file, tolerating a UTF-8 byte order mark (hand-edited files).
fn read_config_text(path: &Path) -> Result<String, PdfError> {
    let bytes = std::fs::read(path).map_err(PdfError::from_io)?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
    Ok(text)
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<serde_json::Value, PdfError> {
    let path = config_dir(&app)?.join("settings.json");
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = read_config_text(&path)?;
    serde_json::from_str(&text).map_err(|e| PdfError::Internal(format!("settings parse error: {e}")))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("settings.json");
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|e| PdfError::Internal(format!("settings serialize error: {e}")))?;
    std::fs::write(&path, text).map_err(PdfError::from_io)
}

#[tauri::command]
pub fn load_recent(app: AppHandle) -> Result<Vec<RecentEntry>, PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = read_config_text(&path)?;
    let mut entries: Vec<RecentEntry> = serde_json::from_str(&text).unwrap_or_default();
    entries.retain(|e| Path::new(&e.path).exists());
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
    entries.truncate(30);
    Ok(entries)
}

#[tauri::command]
pub fn add_recent(app: AppHandle, entry: RecentEntry) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    let mut entries: Vec<RecentEntry> = if path.exists() {
        read_config_text(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    entries.retain(|e| e.path != entry.path);
    entries.insert(0, entry);
    entries.truncate(30);
    let text = serde_json::to_string(&entries)
        .map_err(|e| PdfError::Internal(format!("recent serialize error: {e}")))?;
    std::fs::write(&path, text).map_err(PdfError::from_io)
}

#[tauri::command]
pub fn clear_recent(app: AppHandle) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    if path.exists() {
        std::fs::remove_file(&path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

/// Checks whether an output path exists (drives the overwrite dialog).
#[tauri::command]
pub fn output_exists(path: String) -> bool {
    Path::new(&path).exists()
}

/// Creates a directory (including its parents). The Android shell uses this to
/// stage documents picked through the system file picker inside the app cache.
#[tauri::command]
pub fn ensure_dir(path: String) -> Result<(), PdfError> {
    std::fs::create_dir_all(Path::new(&path)).map_err(PdfError::from_io)
}

/// Suggests a default output path next to the input.
#[tauri::command]
pub fn suggest_output(input: String, suffix: String) -> String {
    pdfcore::docutil::default_output_for(Path::new(&input), &suffix)
        .display()
        .to_string()
}

/// File sizes for the selected files (used by the file list UI).
#[tauri::command]
pub fn file_sizes(paths: Vec<String>) -> Vec<Option<u64>> {
    paths
        .iter()
        .map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .collect()
}

/// Development/screenshot helper: lets an automated run start on a specific
/// screen with pre-selected files, e.g.
///   set PDFSAK_START_SCREEN=compress && set PDFSAK_DEV_FILES=C:\in.pdf
/// Returns empty values in normal use.
#[tauri::command]
pub fn dev_launch_context() -> serde_json::Value {
    serde_json::json!({
        "startScreen": std::env::var("PDFSAK_START_SCREEN").ok(),
        "newTab": std::env::var("PDFSAK_DEV_NEW").ok(),
        "autoRun": std::env::var("PDFSAK_DEV_RUN").is_ok(),
        "tab": std::env::var("PDFSAK_DEV_TAB").ok(),
        "files": std::env::var("PDFSAK_DEV_FILES").ok().map(|value| {
            value
                .split(';')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect::<Vec<String>>()
        }),
    })
}

/// Frontend error sink (production-safe diagnostic log). Only error text and
/// stack traces are recorded - never document content or passwords.
#[tauri::command]
pub fn log_frontend(app: AppHandle, level: String, message: String) {
    let Ok(dir) = app.path().app_log_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("frontend.log");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{timestamp}] [{level}] {message}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

// ---------------------------------------------------------------------------
// AI library and operation log (persistent stores)
// ---------------------------------------------------------------------------

use crate::library::{self, AiLibraryEntry, OperationEntry};

fn ai_library_index(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join("ai-library.json"))
}

fn operations_log(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join("operations.json"))
}

/// Default AI library folder: Documents/PDF Swiss Army Knife AI.
fn default_library_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .document_dir()
        .map(|dir| dir.join("PDF Swiss Army Knife AI"))
        .unwrap_or_else(|_| std::env::temp_dir().join("pdfsak-ai-library"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiEntryRequest {
    pub kind: String,
    pub source_path: String,
    pub source_name: String,
    pub model: String,
    pub pages: u32,
    pub characters: u64,
    #[serde(default)]
    pub options: String,
    pub text: String,
    #[serde(default)]
    pub elapsed_ms: u64,
    /// Overrides the default library folder for this save.
    #[serde(default)]
    pub directory: Option<String>,
}

/// Stores an AI result as Markdown plus an index entry (auto-save and the
/// explicit "save to library" action both use this).
#[tauri::command]
pub fn ai_library_save(app: AppHandle, request: SaveAiEntryRequest) -> Result<AiLibraryEntry, PdfError> {
    let index = ai_library_index(&app)?;
    let dir = request
        .directory
        .as_ref()
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| default_library_dir(&app));
    library::save_ai_entry(
        &index,
        &dir,
        &request.kind,
        &request.source_path,
        &request.source_name,
        &request.model,
        request.pages,
        request.characters,
        &request.options,
        &request.text,
        request.elapsed_ms,
    )
}

#[tauri::command]
pub fn ai_library_list(app: AppHandle) -> Result<Vec<AiLibraryEntry>, PdfError> {
    Ok(library::list_ai_entries(&ai_library_index(&app)?))
}

#[tauri::command]
pub fn ai_library_text(app: AppHandle, id: String) -> Result<String, PdfError> {
    library::read_ai_entry_text(&ai_library_index(&app)?, &id)
}

#[tauri::command]
pub fn ai_library_delete(app: AppHandle, id: String, delete_file: Option<bool>) -> Result<Vec<AiLibraryEntry>, PdfError> {
    let index = ai_library_index(&app)?;
    library::delete_ai_entry(&index, &id, delete_file.unwrap_or(true))?;
    Ok(library::list_ai_entries(&index))
}

#[tauri::command]
pub fn ai_library_clear(app: AppHandle, delete_files: Option<bool>) -> Result<(), PdfError> {
    library::clear_ai_entries(&ai_library_index(&app)?, delete_files.unwrap_or(true))
}

/// Copies a stored result to a user-chosen path (Save as...).
#[tauri::command]
pub fn ai_library_export(app: AppHandle, id: String, target: String) -> Result<String, PdfError> {
    let text = library::read_ai_entry_text(&ai_library_index(&app)?, &id)?;
    let path = pdfcore::docutil::resolve_output_path(
        Path::new(&target),
        pdfcore::docutil::OverwritePolicy::UniqueName,
    )?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
        }
    }
    std::fs::write(&path, text).map_err(PdfError::from_io)?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn ai_library_default_dir(app: AppHandle) -> String {
    default_library_dir(&app).display().to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogRequest {
    pub operation: String,
    pub input_path: String,
    #[serde(default)]
    pub output_path: String,
    #[serde(default)]
    pub page_count: Option<u32>,
    #[serde(default)]
    pub input_bytes: Option<u64>,
    #[serde(default)]
    pub output_bytes: Option<u64>,
    #[serde(default = "default_true")]
    pub ok: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[tauri::command]
pub fn log_operation(app: AppHandle, entry: OperationLogRequest) -> Result<(), PdfError> {
    library::append_operation(
        &operations_log(&app)?,
        OperationEntry {
            id: String::new(),
            created_at: 0,
            operation: entry.operation,
            input_path: entry.input_path,
            output_path: entry.output_path,
            page_count: entry.page_count,
            input_bytes: entry.input_bytes,
            output_bytes: entry.output_bytes,
            ok: entry.ok,
            detail: entry.detail,
        },
    )
}

#[tauri::command]
pub fn load_operations(app: AppHandle) -> Vec<OperationEntry> {
    operations_log(&app)
        .map(|path| library::list_operations(&path))
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_operations(app: AppHandle) -> Result<(), PdfError> {
    library::clear_operations(&operations_log(&app)?)
}

// ---------------------------------------------------------------------------
// Helpers for the async command wrappers
// ---------------------------------------------------------------------------

fn emit_progress_simple(
    progress: &pdfcore::progress::ProgressCallback,
    stage: &str,
    current: u64,
    total: u64,
) {
    progress(pdfcore::progress::ProgressEvent::new(stage, current, total));
}

/// Runs a pdfcore operation on the blocking pool with progress + cancel wired
/// to the job registry.
pub(crate) async fn operation_with_progress<T, F>(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    job_id: Option<String>,
    work: F,
) -> Result<T, PdfError>
where
    T: Send + 'static,
    F: FnOnce(&pdfcore::progress::ProgressCallback, &CancelToken) -> Result<T, PdfError> + Send + 'static,
{
    let job_id = job_id.unwrap_or_else(|| format!("job-{}", uuid::Uuid::new_v4()));
    let cancel = registry.register(&job_id);
    let app_for_progress = app.clone();
    let job_for_progress = job_id.clone();
    let result = run_blocking(move || {
        let progress = move |event: pdfcore::progress::ProgressEvent| {
            emit_progress(&app_for_progress, &job_for_progress, &event);
        };
        work(&progress, &cancel)
    })
    .await;
    registry.finish(&job_id);
    result
}

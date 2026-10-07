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
    let _permit = crate::concurrency::acquire().await;
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
        let path = crate::paths::output_file(&self.path)?;
        Ok((path.into_path_buf(), policy(&self.overwrite)))
    }

    /// Validates the input file for commands that carry one alongside the
    /// output spec. Centralized so every such command uses the same rules.
    #[allow(dead_code)]
    pub(crate) fn input(raw: &str) -> Result<PathBuf, PdfError> {
        Ok(crate::paths::input_file(raw)?.into_path_buf())
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
        name: "OmniOffice".to_string(),
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
    let path = crate::paths::input_file(&path)?.into_path_buf();
    run_blocking(move || pdfcore::info::pdf_info(&path, password.as_deref())).await
}

#[tauri::command]
pub async fn page_thumbnail(
    path: String,
    page: u32,
    max_width: Option<u32>,
    password: Option<String>,
) -> Result<Thumbnail, PdfError> {
    let path = crate::paths::input_file(&path)?.into_path_buf();
    run_blocking(move || {
        let max_width = max_width.unwrap_or(220).clamp(60, 3000);
        let options = pdfcore::render::RenderOptions { dpi: 96.0, max_width: Some(max_width), max_height: None };
        let rendered = pdfcore::render::render_page(&path, password.as_deref(), page, &options)?;
        let bytes = pdfcore::images::encode_image(&rendered, pdfcore::images::ImageFormat::Jpeg, 82, false)?;
        let data_url = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
        Ok(Thumbnail { data_url, width: rendered.width, height: rendered.height })
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
    let path = crate::paths::input_file(&path)?.into_path_buf();
    run_blocking(move || {
        let max_width = max_width.unwrap_or(1100).clamp(200, 4000);
        // Render at a high dpi and let `max_width` cap the result: pdfium's
        // target width is the min of the two, so the bitmap actually reaches
        // the requested raster width. The old fixed 96 dpi could only ever
        // downscale, so a reader zoom returned the same small bitmap and the
        // webview upscaled it (the blurry zoom on Android).
        let options = pdfcore::render::RenderOptions { dpi: 600.0, max_width: Some(max_width), max_height: None };
        let rendered = pdfcore::render::render_page(&path, password.as_deref(), page, &options)?;
        preview_data_url(&rendered, format.as_deref(), quality)
    })
    .await
}

/// Encodes a preview or tile as a data URL. Reading mode requests JPEG (much
/// smaller for large pages); the thumbnail/preview default stays lossless PNG.
fn preview_data_url(
    rendered: &pdfcore::render::RenderedPage,
    format: Option<&str>,
    quality: Option<u8>,
) -> Result<Thumbnail, PdfError> {
    let (bytes, mime) = match format {
        Some("jpeg") | Some("jpg") => (
            pdfcore::images::encode_image(rendered, pdfcore::images::ImageFormat::Jpeg, quality.unwrap_or(86), false)?,
            "image/jpeg",
        ),
        _ => (pdfcore::images::encode_image(rendered, pdfcore::images::ImageFormat::Png, 90, false)?, "image/png"),
    };
    let data_url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
    Ok(Thumbnail { data_url, width: rendered.width, height: rendered.height })
}

/// Largest tile edge `page_tile` renders, in pixels.
const MAX_TILE_EDGE: u32 = 1024;

/// Highest tile scale in output pixels per PDF point: the reader's 400 % zoom
/// (96/72 CSS px per point at 100 %) at its 2.5 device pixel ratio cap.
const MAX_TILE_SCALE: f32 = 4.0 * (96.0 / 72.0) * 2.5;

/// Renders one tile of a page for the reader's high-zoom overlay: the region
/// `x, y, width, height` (output pixels) of the page as if it were rendered
/// whole at `scale` pixels per point. Past the full-page preview's raster cap
/// the reader keeps that preview as a base layer and lays these tiles over the
/// part of the page on screen. The open document is reused between tiles.
#[tauri::command]
pub async fn page_tile(
    path: String,
    page: u32,
    scale: f32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    password: Option<String>,
    format: Option<String>,
    quality: Option<u8>,
) -> Result<Thumbnail, PdfError> {
    let path = crate::paths::input_file(&path)?.into_path_buf();
    // Unlike the preview width, a tile's scale and position are not clamped:
    // a clamped tile would no longer line up with the page underneath it. The
    // small slack absorbs the frontend computing the same bound in f64.
    if !scale.is_finite() || scale <= 0.0 || scale > MAX_TILE_SCALE * 1.001 {
        return Err(PdfError::InvalidInput(format!("tile scale out of range: {scale}")));
    }
    if width == 0 || height == 0 || width > MAX_TILE_EDGE || height > MAX_TILE_EDGE {
        return Err(PdfError::InvalidInput(format!(
            "tile must be 1-{MAX_TILE_EDGE} px per side, got {width}x{height}"
        )));
    }
    run_blocking(move || {
        let region = pdfcore::render::PixelRegion { x, y, width, height };
        let rendered = pdfcore::render::render_page_region(&path, password.as_deref(), page, scale, region)?;
        preview_data_url(&rendered, format.as_deref(), quality)
    })
    .await
}

/// Extracts the text layer of a single page (reading mode: "copy page text").
#[tauri::command]
pub async fn page_text(path: String, page: u32, password: Option<String>) -> Result<String, PdfError> {
    let path = crate::paths::input_file(&path)?.into_path_buf();
    run_blocking(move || pdfcore::render::extract_page_text(&path, password.as_deref(), page)).await
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
    let path = crate::paths::input_file(&path)?.into_path_buf();
    operation_with_progress(app, registry, job_id, move |progress, cancel| {
        let result = pdfcore::render::search_document(
            &path,
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
    let path = crate::paths::input_file(&path)?.into_path_buf();
    run_blocking(move || pdfcore::security::check_password(&path, &password)).await
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
        let inputs: Vec<PathBuf> = request
            .inputs
            .iter()
            .map(|input| crate::paths::input_file(input).map(|path| path.into_path_buf()))
            .collect::<Result<_, _>>()?;
        if inputs.len() < 2 {
            return Err(PdfError::InvalidInput("select at least two PDF files".into()));
        }
        let (path, pages) = pdfcore::merge::merge_files(
            &inputs,
            &output,
            &pdfcore::merge::MergeOptions { preserve_metadata: request.preserve_metadata },
            policy,
            &progress,
            &cancel,
        )?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(pages),
            original_bytes: inputs
                .iter()
                .filter_map(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .reduce(|a, b| a + b),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await;
    registry.complete(&job_id, result.is_ok());
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
        let input = crate::paths::input_file(&request.input)?;
        validate_pages(input.as_path(), &request)?;
        emit_progress_simple(progress, "extract", 0, 1);
        let path = pdfcore::organize::extract_pages(
            input.as_path(),
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
            original_bytes: std::fs::metadata(input.as_path()).ok().map(|m| m.len()),
            output_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            reduction: None,
            message: None,
        })
    })
    .await
}

fn validate_pages(input: &Path, request: &PagesRequest) -> Result<Vec<u32>, PdfError> {
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
        let input = crate::paths::input_file(&request.input)?;
        let pages = validate_pages(input.as_path(), &request)?;
        let path =
            pdfcore::organize::delete_pages(input.as_path(), &pages, &output, policy, request.password.as_deref())?;
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
        let input = crate::paths::input_file(&request.input)?;
        let pages = validate_pages(input.as_path(), &request)?;
        let degrees = request.degrees.unwrap_or(90);
        let path = pdfcore::organize::rotate_pages(
            input.as_path(),
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
        let input = crate::paths::input_file(&request.input)?;
        emit_progress_simple(progress, "organize", 0, 1);
        let path = pdfcore::organize::apply_page_plan(
            input.as_path(),
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
            original_bytes: std::fs::metadata(input.as_path()).ok().map(|m| m.len()),
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
        let input = crate::paths::input_file(&request.input)?;
        let output_dir = crate::paths::directory(&request.output_dir)?;
        let parts = pdfcore::organize::split_pdf(
            input.as_path(),
            &request.mode,
            output_dir.as_path(),
            policy(&request.overwrite),
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        Ok(SplitResponse { parts, output_dir: request.output_dir.clone() })
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
        let input = crate::paths::input_file(&input)?;
        pdfcore::compress::estimate_compression(input.as_path(), &options, password.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn compress_pdf(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: CompressRequest,
) -> Result<OpResult, PdfError> {
    operation_with_progress(app, registry, request.job_id.clone(), move |progress, cancel| {
        let (output, policy) = request.output.resolve()?;
        let input = crate::paths::input_file(&request.input)?;
        let result = pdfcore::compress::compress_pdf(
            input.as_path(),
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
        let input = crate::paths::input_file(&request.input)?;
        let result = pdfcore::ocr::ocr_pdf(
            input.as_path(),
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
            original_bytes: std::fs::metadata(input.as_path()).ok().map(|m| m.len()),
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
        let input = crate::paths::input_file(&request.input)?;
        let path = pdfcore::security::protect_pdf(
            input.as_path(),
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
        let input = crate::paths::input_file(&request.input)?;
        let path = pdfcore::security::unlock_pdf(input.as_path(), &output, &request.password, policy)?;
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
        let input = crate::paths::input_file(&request.input)?;
        let output_dir = crate::paths::directory(&request.output_dir)?;
        let result = pdfcore::convert::pdf_to_images(
            input.as_path(),
            output_dir.as_path(),
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
        let items: Vec<pdfcore::images::ImageItem> = request
            .items
            .iter()
            .map(|item| {
                Ok(pdfcore::images::ImageItem {
                    path: crate::paths::input_file(&item.path)?.into_path_buf().to_string_lossy().to_string(),
                    rotation_delta: item.rotation_delta,
                })
            })
            .collect::<Result<_, PdfError>>()?;
        let path = pdfcore::images::images_to_pdf(&items, &request.options, &output, policy, progress, cancel)?;
        Ok(OpResult {
            path: path.display().to_string(),
            page_count: Some(items.len() as u32),
            original_bytes: items
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
        let input = crate::paths::input_file(&request.input)?;
        let path = pdfcore::pagelayout::resize_pages(
            input.as_path(),
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
        let input = crate::paths::input_file(&request.input)?;
        let path = pdfcore::pagelayout::crop_pages(
            input.as_path(),
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
    /// Keep existing signatures by appending the change as a new revision
    /// (default on). Removing metadata cannot be expressed that way, so it
    /// always rewrites - and says so in the result message.
    #[serde(default = "default_true")]
    pub keep_signatures: bool,
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
        let input = crate::paths::input_file(&request.input)?;
        let bytes = crate::paths::read_input_file(&request.input, 2u64 * 1024 * 1024 * 1024)?;
        let signatures = pdfcore::incremental::signature_count(&bytes);

        // Appending a revision keeps every signature meaningful: each one still
        // covers exactly the bytes it signed. A removal cannot be expressed as
        // an appended revision, so it falls back to a rewrite (and reports what
        // that cost).
        if signatures > 0 && request.keep_signatures && !request.remove {
            let updated = pdfcore::metadata::edit_metadata_incremental(&bytes, &request.metadata)?;
            let path = pdfcore::docutil::resolve_output_path(&output, policy)?;
            pdfcore::docutil::write_bytes_atomic(&path, &updated)?;
            return Ok(OpResult {
                path: path.display().to_string(),
                page_count: None,
                original_bytes: Some(bytes.len() as u64),
                output_bytes: Some(updated.len() as u64),
                reduction: None,
                message: Some(format!(
                    "{signatures} signature(s) preserved - the change was appended as a new revision"
                )),
            });
        }

        let path = pdfcore::metadata::edit_metadata_file(
            input.as_path(),
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
            message: if signatures > 0 {
                Some(format!("{signatures} signature(s) invalidated - this change rewrites the document"))
            } else {
                None
            },
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
    /// Keep existing signatures by appending the numbers as a new revision
    /// (default on).
    #[serde(default = "default_true")]
    pub keep_signatures: bool,
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
        let input = crate::paths::input_file(&request.input)?;
        let bytes = crate::paths::read_input_file(&request.input, 2u64 * 1024 * 1024 * 1024)?;
        let signatures = pdfcore::incremental::signature_count(&bytes);

        // Page numbers are additive: appending them as a new revision keeps
        // every existing signature valid.
        if signatures > 0 && request.keep_signatures {
            let updated = pdfcore::numbering::numbering_pdf_incremental(&bytes, &request.options)?;
            let path = pdfcore::docutil::resolve_output_path(&output, policy)?;
            pdfcore::docutil::write_bytes_atomic(&path, &updated)?;
            return Ok(OpResult {
                path: path.display().to_string(),
                page_count: None,
                original_bytes: Some(bytes.len() as u64),
                output_bytes: Some(updated.len() as u64),
                reduction: None,
                message: Some(format!(
                    "{signatures} signature(s) preserved - the page numbers were appended as a new revision"
                )),
            });
        }

        let path = pdfcore::numbering::add_page_numbers(
            input.as_path(),
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
            message: if signatures > 0 {
                Some(format!("{signatures} signature(s) invalidated - this change rewrites the document"))
            } else {
                None
            },
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
    /// Keep existing signatures by appending the watermark as a new revision
    /// (default on).
    #[serde(default = "default_true")]
    pub keep_signatures: bool,
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
        let input = crate::paths::input_file(&request.input)?;
        let bytes = crate::paths::read_input_file(&request.input, 2u64 * 1024 * 1024 * 1024)?;
        let signatures = pdfcore::incremental::signature_count(&bytes);

        // A watermark is additive: appending it as a new revision keeps every
        // existing signature valid and visible as "what was signed".
        if signatures > 0 && request.keep_signatures {
            let updated = pdfcore::watermark::watermark_pdf_incremental(&bytes, &request.options)?;
            let path = pdfcore::docutil::resolve_output_path(&output, policy)?;
            pdfcore::docutil::write_bytes_atomic(&path, &updated)?;
            return Ok(OpResult {
                path: path.display().to_string(),
                page_count: None,
                original_bytes: Some(bytes.len() as u64),
                output_bytes: Some(updated.len() as u64),
                reduction: None,
                message: Some(format!(
                    "{signatures} signature(s) preserved - the watermark was appended as a new revision"
                )),
            });
        }

        let path = pdfcore::watermark::add_watermark(
            input.as_path(),
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
            message: if signatures > 0 {
                Some(format!("{signatures} signature(s) invalidated - this change rewrites the document"))
            } else {
                None
            },
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
    /// Keep existing signatures by appending the stamp as a new revision
    /// (default on).
    #[serde(default = "default_true")]
    pub keep_signatures: bool,
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
        let input = crate::paths::input_file(&request.input)?;
        let bytes = crate::paths::read_input_file(&request.input, 2u64 * 1024 * 1024 * 1024)?;
        let signatures = pdfcore::incremental::signature_count(&bytes);

        // A stamp is additive: appending it as a new revision keeps every
        // existing signature valid and visible as "what was signed".
        if signatures > 0 && request.keep_signatures {
            let updated = pdfcore::annotate::annotate_pdf_incremental(&bytes, &request.annotations)?;
            let path = pdfcore::docutil::resolve_output_path(&output, policy)?;
            pdfcore::docutil::write_bytes_atomic(&path, &updated)?;
            return Ok(OpResult {
                path: path.display().to_string(),
                page_count: None,
                original_bytes: Some(bytes.len() as u64),
                output_bytes: Some(updated.len() as u64),
                reduction: None,
                message: Some(format!(
                    "{signatures} signature(s) preserved - the stamp was appended as a new revision"
                )),
            });
        }

        let path = pdfcore::annotate::annotate_pdf(
            input.as_path(),
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
            message: if signatures > 0 {
                Some(format!("{signatures} signature(s) invalidated - this change rewrites the document"))
            } else {
                None
            },
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
        let input = crate::paths::input_file(&request.input)?;
        let result = pdfcore::redact::redact_pdf(
            input.as_path(),
            &output,
            &request.areas,
            &request.options,
            policy,
            request.password.as_deref(),
            progress,
            cancel,
        )?;
        // Surface the real verification outcome. A redaction that still leaves
        // selected values extractable must never look like a clean success.
        let mut notes: Vec<String> = Vec::new();
        if result.characters_removed == 0 && result.unmatched_areas > 0 {
            notes.push(format!("{} area(s) did not match any text.", result.unmatched_areas));
        }
        if let Some(remaining) = result.remaining_matches.first() {
            notes.push(format!(
                "WARNING: {} selected value(s) are still extractable after redaction (e.g. {}). The output is not safe to share.",
                result.remaining_matches.len(),
                remaining
            ));
        } else if !result.verification_message.is_empty() {
            notes.push(result.verification_message.clone());
        }
        let message = if notes.is_empty() { None } else { Some(notes.join(" ")) };
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
        let path = crate::paths::input_file(&path)?;
        let cancel = CancelToken::new();
        let (pages, _) =
            pdfcore::textbox::page_chars(path.as_path(), password.as_deref(), &[page], 1, &cancel, &|_, _| {})?;
        let chars =
            pages.get(&page).ok_or_else(|| PdfError::Internal(format!("page {page} has no extractable text")))?;
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
        let left = crate::paths::input_file(&request.left)?;
        let right = crate::paths::input_file(&request.right)?;
        pdfcore::compare::compare_pdfs(
            left.as_path(),
            right.as_path(),
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
    run_blocking(move || {
        let path = crate::paths::input_file(&path)?;
        pdfcore::inspect::inspect_document(path.as_path(), password.as_deref())
    })
    .await
}

// ---------------------------------------------------------------------------
// Settings / recent files (paths + timestamps only, never content)
// ---------------------------------------------------------------------------

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app.path().app_config_dir().map_err(|e| PdfError::Internal(format!("config dir unavailable: {e}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

/// Reads a text file, tolerating a UTF-8 byte order mark (hand-edited files).
fn read_config_text(path: &Path) -> Result<String, PdfError> {
    let bytes = std::fs::read(path).map_err(PdfError::from_io)?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
    Ok(text)
}

/// Shared crash-safe write for the small JSON stores (settings, recents, AI
/// settings/library/logs, secrets): temp sibling + fsync + atomic rename.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PdfError> {
    officecore::io::write_atomic(path, bytes)
        .map_err(|error| PdfError::Internal(format!("could not write {}: {error}", path.display())))
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
    write_atomic(&path, text.as_bytes())
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
        read_config_text(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    } else {
        Vec::new()
    };
    entries.retain(|e| e.path != entry.path);
    entries.insert(0, entry);
    entries.truncate(30);
    let text =
        serde_json::to_string(&entries).map_err(|e| PdfError::Internal(format!("recent serialize error: {e}")))?;
    write_atomic(&path, text.as_bytes())
}

#[tauri::command]
pub fn clear_recent(app: AppHandle) -> Result<(), PdfError> {
    let path = config_dir(&app)?.join("recent.json");
    if path.exists() {
        std::fs::remove_file(&path).map_err(PdfError::from_io)?;
    }
    Ok(())
}

/// Checks whether an output path exists (drives the overwrite dialog). This is
/// a probe for *absence*, so only the path shape is validated.
#[tauri::command]
pub fn output_exists(path: String) -> bool {
    crate::paths::lexical(&path).map(|validated| validated.as_path().exists()).unwrap_or(false)
}

/// Creates a directory (including its parents). The Android shell uses this to
/// stage documents picked through the system file picker inside the app cache.
///
/// The directory intentionally does not exist yet, so this validates the path's
/// shape only (absolute, no `..`, no reserved names) instead of requiring it.
#[tauri::command]
pub fn ensure_dir(path: String) -> Result<(), PdfError> {
    let path = crate::paths::lexical(&path)?;
    std::fs::create_dir_all(path.as_path()).map_err(PdfError::from_io)
}

/// Suggests a default output path next to the input.
#[tauri::command]
pub fn suggest_output(input: String, suffix: String) -> String {
    let Ok(input) = crate::paths::input_file(&input) else {
        return String::new();
    };
    pdfcore::docutil::default_output_for(input.as_path(), &suffix).display().to_string()
}

/// File sizes for the selected files (used by the file list UI). Missing or
/// invalid paths are reported as `None` instead of failing the whole call.
#[tauri::command]
pub fn file_sizes(paths: Vec<String>) -> Vec<Option<u64>> {
    paths
        .iter()
        .map(|p| {
            crate::paths::input_file(p)
                .ok()
                .and_then(|validated| std::fs::metadata(validated.as_path()).ok())
                .map(|m| m.len())
        })
        .collect()
}

/// A file fingerprint used to detect that a document changed on disk since it
/// was opened (another program, another tab, or a cloud client). `sha256` is
/// lowercase hex; `size` and `modified_ms` let the caller short-circuit the
/// hash when only the timestamp moved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFingerprint {
    pub exists: bool,
    pub size: u64,
    pub modified_ms: i64,
    pub sha256: String,
}

/// Fingerprints a local file for the file-conflict check. A missing file is
/// reported as `exists: false` rather than an error, so the caller can tell
/// "deleted outside the app" apart from "check failed".
#[tauri::command]
pub fn file_fingerprint(path: String) -> Result<FileFingerprint, PdfError> {
    let validated = crate::paths::lexical(&path)?;
    let target = validated.as_path();
    match std::fs::metadata(target) {
        Err(_) => Ok(FileFingerprint { exists: false, size: 0, modified_ms: 0, sha256: String::new() }),
        Ok(metadata) => {
            if !metadata.is_file() {
                return Ok(FileFingerprint { exists: false, size: 0, modified_ms: 0, sha256: String::new() });
            }
            let bytes = std::fs::read(target).map_err(PdfError::from_io)?;
            let digest = <sha2::Sha256 as sha2::Digest>::digest(&bytes);
            let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or(0);
            Ok(FileFingerprint { exists: true, size: metadata.len(), modified_ms, sha256 })
        }
    }
}

// ---------------------------------------------------------------------------
// OS integration (open / reveal)
// ---------------------------------------------------------------------------

/// Extensions the OS open/reveal commands accept: only formats the app itself
/// handles as documents. A compromised renderer therefore cannot use these
/// commands to launch an executable or script through the default handler.
/// `opener` permissions are not granted to the webview at all; it reaches the
/// OS only through the two validated commands below.
const OPENABLE_EXTENSIONS: &[&str] = &[
    "pdf", "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "gif", "txt", "md", "csv", "json", "docx", "odt", "rtf",
    "xlsx", "ods", "tsv", "pptx", "odp", "oswk", "osed", "ospr", "osdt",
];

fn openable_document_path(raw: &str) -> Result<PathBuf, PdfError> {
    let path = crate::paths::input_file(raw)?;
    let extension = path
        .as_path()
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !OPENABLE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(PdfError::InvalidInput(format!("refusing to hand this file type to the operating system: {raw}")));
    }
    Ok(path.into_path_buf())
}

/// Opens a document the app produced with the OS default handler.
#[tauri::command]
pub fn open_document_file(path: String) -> Result<(), PdfError> {
    let path = openable_document_path(&path)?;
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|error| PdfError::Internal(format!("could not open the file: {error}")))
}

/// Reveals a document in the system file manager.
#[tauri::command]
pub fn reveal_document_file(path: String) -> Result<(), PdfError> {
    let path = openable_document_path(&path)?;
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|error| PdfError::Internal(format!("could not reveal the file: {error}")))
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
    let timestamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
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

/// Default AI library folder: `Documents/OmniOffice AI` on desktop. A library
/// created under the old product name (`Documents/PDF Swiss Army Knife AI`) is
/// reused until the new folder exists, so the rename does not orphan entries.
/// Android keeps the files inside the app data directory instead: the
/// Documents resolver there points at app-private external storage, and the
/// SAF publishing flow copies results out to a location the user picks.
fn default_library_dir(app: &AppHandle) -> PathBuf {
    #[cfg(target_os = "android")]
    if let Ok(dir) = app.path().app_data_dir() {
        return dir.join("ai-library");
    }
    if let Ok(documents) = app.path().document_dir() {
        let current = documents.join("OmniOffice AI");
        if !current.exists() {
            let legacy = documents.join("PDF Swiss Army Knife AI");
            if legacy.is_dir() {
                return legacy;
            }
        }
        return current;
    }
    std::env::temp_dir().join("pdfsak-ai-library")
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
    // The library folder may be created below, so validate its shape only.
    let dir = match request.directory.as_deref() {
        Some(raw) if !raw.is_empty() => crate::paths::lexical(raw)?.into_path_buf(),
        _ => default_library_dir(&app),
    };
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
pub fn ai_library_delete(
    app: AppHandle,
    id: String,
    delete_file: Option<bool>,
) -> Result<Vec<AiLibraryEntry>, PdfError> {
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
    let target = crate::paths::output_file(&target)?;
    let path = pdfcore::docutil::resolve_output_path(target.as_path(), pdfcore::docutil::OverwritePolicy::UniqueName)?;
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
    operations_log(&app).map(|path| library::list_operations(&path)).unwrap_or_default()
}

#[tauri::command]
pub fn clear_operations(app: AppHandle) -> Result<(), PdfError> {
    library::clear_operations(&operations_log(&app)?)
}

// ---------------------------------------------------------------------------
// Helpers for the async command wrappers
// ---------------------------------------------------------------------------

fn emit_progress_simple(progress: &pdfcore::progress::ProgressCallback, stage: &str, current: u64, total: u64) {
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
    registry.complete(&job_id, result.is_ok());
    result
}

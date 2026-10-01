//! AI assistant commands backed by the DeepSeek API.
//!
//! Privacy model (important):
//! * This is the only part of the application that uses the network.
//! * Nothing is sent unless the user explicitly runs an AI action.
//! * Only the extracted document text is transmitted - never the file itself,
//!   never passwords, never metadata beyond what the prompt needs.
//! * The API key is stored with DPAPI on Windows and is never logged.
//!
//! Every action emits `ai:progress` events and streams the answer through
//! `ai:chunk` events so the UI can render text while it is generated.

use crate::jobs::JobRegistry;
use crate::secret;
use aicore::prompts::{self, SummaryOptions, TranslateOptions};
use aicore::{AiConfig, AiError, CancelToken, ChatMessage, ChatOptions, DeepSeekClient};
use pdfcore::error::{ErrorCode, PdfError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

/// Safety cap for pathological documents; the real limit is the configured
/// context budget (up to 1M tokens).
const MAX_PAGES_FOR_CONTEXT: u32 = 2_000;
const MIN_TEXT_CHARS: usize = 20;

fn ai_error(error: AiError) -> PdfError {
    let code = match error {
        AiError::MissingApiKey => ErrorCode::AiNotConfigured,
        AiError::InvalidApiKey(_) => ErrorCode::AiInvalidKey,
        AiError::RateLimited(_) => ErrorCode::AiRateLimited,
        AiError::InsufficientBalance(_) => ErrorCode::AiInsufficientBalance,
        AiError::Network(_) => ErrorCode::AiNetwork,
        AiError::Cancelled => ErrorCode::Cancelled,
        AiError::NoText => ErrorCode::AiNoText,
        AiError::TooLarge => ErrorCode::AiTooLarge,
        AiError::Server(_, _) => ErrorCode::AiServerError,
        AiError::InvalidResponse(_) => ErrorCode::AiInvalidResponse,
        AiError::ProviderUnreachable(_) => ErrorCode::AiNetwork,
        AiError::InvalidBaseUrl(_) => ErrorCode::InvalidInput,
        AiError::Unsupported(_) => ErrorCode::Unsupported,
    };
    PdfError::coded(code, error.to_string())
}

// ---------------------------------------------------------------------------
// Settings & key storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsFile {
    #[serde(default = "default_base_url", alias = "base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens", alias = "max_tokens")]
    pub max_tokens: u32,
    /// DeepSeek V4 thinking mode (only sent for deepseek-v4-* models).
    #[serde(default = "default_thinking", alias = "thinking")]
    pub thinking: bool,
    /// "low" | "high" | "max"
    #[serde(default = "default_reasoning_effort", alias = "reasoning_effort")]
    pub reasoning_effort: String,
    /// Input budget in tokens (DeepSeek V4 allows 1M).
    #[serde(default = "default_context_tokens", alias = "context_tokens")]
    pub context_tokens: u32,
    /// Provider id: "deepseek", "open_ai_compatible", "ollama", "gemini", "custom".
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Optional embedding model (providers with embedding capability).
    #[serde(default, alias = "embedding_model")]
    pub embedding_model: Option<String>,
}

fn default_provider() -> String {
    "deepseek".to_string()
}

fn default_context_tokens() -> u32 {
    aicore::default_context_tokens()
}

fn default_thinking() -> bool {
    true
}

fn default_reasoning_effort() -> String {
    "high".to_string()
}

fn default_base_url() -> String {
    aicore::DEFAULT_BASE_URL.to_string()
}
fn default_model() -> String {
    aicore::DEFAULT_MODEL.to_string()
}
fn default_temperature() -> f32 {
    0.2
}
fn default_max_tokens() -> u32 {
    4096
}

impl Default for AiSettingsFile {
    fn default() -> Self {
        Self {
            base_url: aicore::DEFAULT_BASE_URL.to_string(),
            model: aicore::DEFAULT_MODEL.to_string(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            thinking: default_thinking(),
            reasoning_effort: default_reasoning_effort(),
            context_tokens: default_context_tokens(),
            provider: default_provider(),
            embedding_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub configured: bool,
    /// "dpapi" (encrypted), "plain" (no OS encryption available) or "none".
    pub key_storage: String,
    pub masked_key: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking: bool,
    pub reasoning_effort: String,
    pub context_tokens: u32,
    /// Maximum output tokens accepted by the API (384K).
    pub max_output_tokens: u32,
    /// Provider id and its honest one-line privacy note.
    pub provider: String,
    pub provider_label: String,
    pub provider_note: String,
    pub capabilities: aicore::ProviderCapabilities,
    pub embedding_model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsInput {
    #[serde(default)]
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(default)]
    pub thinking: Option<bool>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub context_tokens: Option<u32>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub embedding_model: Option<String>,
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, PdfError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| PdfError::Internal(format!("config dir unavailable: {error}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join("ai.json"))
}

fn key_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join("ai-key.bin"))
}

fn load_settings_file(app: &AppHandle) -> AiSettingsFile {
    settings_path(app)
        .ok()
        .filter(|path| path.exists())
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| {
            // Tolerate a UTF-8 BOM (hand-edited configuration files).
            let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
            text
        })
        .and_then(|text| serde_json::from_str::<AiSettingsFile>(&text).ok())
        .unwrap_or_default()
}

fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() <= 8 {
        return if trimmed.is_empty() {
            String::new()
        } else {
            "••••".into()
        };
    }
    format!("{}••••{}", &trimmed[..4], &trimmed[trimmed.len() - 4..])
}

fn view(app: &AppHandle) -> AiSettingsView {
    let settings = load_settings_file(app);
    let key = key_path(app)
        .ok()
        .and_then(|path| secret::load_api_key(&path).ok())
        .unwrap_or_default();
    let storage = key_path(app)
        .ok()
        .filter(|path| path.exists())
        .map(|path| {
            std::fs::read_to_string(&path)
                .map(|content| {
                    if content.starts_with("dpapi:") {
                        "dpapi".to_string()
                    } else {
                        "plain".to_string()
                    }
                })
                .unwrap_or_else(|_| "plain".to_string())
        })
        .unwrap_or_else(|| "none".to_string());
    let provider = provider_kind(&settings.provider);
    AiSettingsView {
        // A local Ollama server needs no API key, so it counts as configured.
        configured: !key.trim().is_empty() || provider == aicore::ProviderKind::Ollama,
        key_storage: storage,
        masked_key: mask_key(&key),
        base_url: settings.base_url,
        model: settings.model,
        temperature: settings.temperature,
        max_tokens: settings.max_tokens,
        thinking: settings.thinking,
        reasoning_effort: settings.reasoning_effort,
        context_tokens: settings.context_tokens.clamp(8_000, aicore::MAX_CONTEXT_TOKENS),
        max_output_tokens: aicore::MAX_OUTPUT_TOKENS,
        provider: settings.provider.clone(),
        provider_label: provider_kind(&settings.provider).label().to_string(),
        provider_note: aicore::provider_notes(provider_kind(&settings.provider)).to_string(),
        capabilities: aicore::ProviderCapabilities::for_kind(provider_kind(&settings.provider)),
        embedding_model: settings.embedding_model.clone(),
    }
}

fn provider_kind(value: &str) -> aicore::ProviderKind {
    aicore::ProviderKind::parse(value).unwrap_or_default()
}

fn build_config(app: &AppHandle) -> Result<AiConfig, PdfError> {
    let settings = load_settings_file(app);
    let key = key_path(app)
        .ok()
        .and_then(|path| secret::load_api_key(&path).ok())
        .unwrap_or_default();
    let config = AiConfig {
        api_key: key,
        base_url: settings.base_url,
        model: settings.model,
        temperature: settings.temperature,
        max_tokens: aicore::clamp_output_tokens(settings.max_tokens),
        thinking: settings.thinking,
        reasoning_effort: settings.reasoning_effort,
        context_tokens: settings.context_tokens.clamp(8_000, aicore::MAX_CONTEXT_TOKENS),
        provider: provider_kind(&settings.provider),
        embedding_model: settings.embedding_model.clone(),
    };
    if !config.is_configured() {
        return Err(ai_error(AiError::MissingApiKey));
    }
    Ok(config)
}

#[tauri::command]
pub fn ai_get_settings(app: AppHandle) -> AiSettingsView {
    view(&app)
}

#[tauri::command]
pub fn ai_save_settings(app: AppHandle, input: AiSettingsInput) -> Result<AiSettingsView, PdfError> {
    let file = AiSettingsFile {
        base_url: aicore::normalize_base_url(if input.base_url.trim().is_empty() {
            aicore::DEFAULT_BASE_URL
        } else {
            input.base_url.trim()
        })
        .map_err(ai_error)?,
        model: if input.model.trim().is_empty() {
            aicore::DEFAULT_MODEL.to_string()
        } else {
            input.model.trim().to_string()
        },
        temperature: input.temperature.clamp(0.0, 1.5),
        max_tokens: aicore::clamp_output_tokens(input.max_tokens),
        thinking: input.thinking.unwrap_or_else(default_thinking),
        reasoning_effort: input
            .reasoning_effort
            .map(|value| match value.trim().to_lowercase().as_str() {
                "low" => "low".to_string(),
                "max" => "max".to_string(),
                _ => "high".to_string(),
            })
            .unwrap_or_else(default_reasoning_effort),
        context_tokens: input
            .context_tokens
            .unwrap_or_else(default_context_tokens)
            .clamp(8_000, aicore::MAX_CONTEXT_TOKENS),
        provider: input
            .provider
            .map(|value| provider_kind(&value).as_str().to_string())
            .unwrap_or_else(default_provider),
        embedding_model: input.embedding_model.filter(|value| !value.trim().is_empty()),
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|error| PdfError::Internal(format!("settings serialize failed: {error}")))?;
    crate::commands::write_atomic(&settings_path(&app)?, text.as_bytes())?;
    if let Some(key) = input.api_key {
        if !key.trim().is_empty() {
            secret::save_api_key(&key_path(&app)?, &key)?;
        }
    }
    Ok(view(&app))
}

#[tauri::command]
pub fn ai_clear_key(app: AppHandle) -> Result<AiSettingsView, PdfError> {
    secret::delete_api_key(&key_path(&app)?)?;
    Ok(view(&app))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    pub ok: bool,
    pub message: String,
    pub model: String,
}

#[tauri::command]
pub async fn ai_test_connection(app: AppHandle) -> Result<AiTestResult, PdfError> {
    let config = build_config(&app)?;
    let model = config.model.clone();
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    match client.test_connection().await {
        Ok(reply) => Ok(AiTestResult {
            ok: true,
            message: reply,
            model,
        }),
        Err(error) => Ok(AiTestResult {
            ok: false,
            message: ai_error(error).to_string(),
            model,
        }),
    }
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

fn emit_progress(app: &AppHandle, job_id: &str, stage: &str, current: u64, total: u64) {
    let _ = app.emit(
        "ai:progress",
        serde_json::json!({ "jobId": job_id, "stage": stage, "current": current, "total": total }),
    );
}

fn emit_chunk(app: &AppHandle, job_id: &str, delta: &str) {
    let _ = app.emit(
        "ai:chunk",
        serde_json::json!({ "jobId": job_id, "delta": delta, "kind": "content" }),
    );
}

/// Thinking-mode deltas are streamed separately so the UI can show them as a
/// dimmed trace instead of mixing them into the answer.
fn emit_reasoning(app: &AppHandle, job_id: &str, delta: &str) {
    let _ = app.emit(
        "ai:chunk",
        serde_json::json!({ "jobId": job_id, "delta": delta, "kind": "reasoning" }),
    );
}

/// Extracts the page texts of a PDF (and reports progress/cancellation).
fn extract_pages(
    app: &AppHandle,
    job_id: &str,
    path: &Path,
    password: Option<&str>,
    pages: Option<&[u32]>,
    cancel: &CancelToken,
) -> Result<Vec<(u32, String)>, PdfError> {
    let geometries = pdfcore::render::page_geometries(path, password)?;
    let total = geometries.len() as u32;
    if total == 0 {
        return Err(ai_error(AiError::NoText));
    }
    let selected: Vec<u32> = match pages {
        Some(list) if !list.is_empty() => {
            for page in list {
                if *page == 0 || *page > total {
                    return Err(PdfError::RangeOutOfBounds);
                }
            }
            list.to_vec()
        }
        _ => (1..=total.min(MAX_PAGES_FOR_CONTEXT)).collect(),
    };
    let mut out = Vec::with_capacity(selected.len());
    for (index, page) in selected.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(ai_error(AiError::Cancelled));
        }
        emit_progress(app, job_id, "extract", index as u64, selected.len() as u64);
        let text = pdfcore::render::extract_page_text(path, password, *page).unwrap_or_default();
        let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !cleaned.is_empty() {
            out.push((*page, cleaned));
        }
    }
    let characters: usize = out.iter().map(|(_, text)| text.len()).sum();
    if characters < MIN_TEXT_CHARS {
        return Err(ai_error(AiError::NoText));
    }
    Ok(out)
}

fn join_text(pages: &[(u32, String)]) -> String {
    prompts::format_pages(pages, false)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreview {
    pub pages: u32,
    pub characters: u64,
    pub sample: String,
    pub estimated_words: u64,
}

#[tauri::command]
pub async fn ai_document_preview(
    app: AppHandle,
    path: String,
    pages: Option<Vec<u32>>,
    password: Option<String>,
) -> Result<AiPreview, PdfError> {
    let job_id = "ai-preview".to_string();
    let cancel = CancelToken::new();
    let path = crate::paths::input_file(&path)?;
    let extracted = extract_pages(
        &app,
        &job_id,
        path.as_path(),
        password.as_deref(),
        pages.as_deref(),
        &cancel,
    )?;
    let text = join_text(&extracted);
    Ok(AiPreview {
        pages: extracted.len() as u32,
        characters: text.len() as u64,
        sample: text.chars().take(400).collect(),
        estimated_words: (text.len() / 6) as u64,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTextResult {
    pub text: String,
    pub pages: u32,
    pub characters: u64,
    pub model: String,
    pub elapsed_ms: u64,
}

// ---------------------------------------------------------------------------
// Summarize
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSummarizeRequest {
    pub path: String,
    pub options: SummaryOptions,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

#[tauri::command]
pub async fn ai_summarize(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AiSummarizeRequest,
) -> Result<AiTextResult, PdfError> {
    let started = std::time::Instant::now();
    let (cancel, mut job_guard) = registry.register_ai_managed(&request.job_id);
    let config = build_config(&app)?;
    let model = config.model.clone();
    let path = crate::paths::input_file(&request.path)?;
    let extracted = extract_pages(
        &app,
        &request.job_id,
        path.as_path(),
        request.password.as_deref(),
        request.pages.as_deref(),
        &cancel,
    )?;
    let text = join_text(&extracted);
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let plan = prompts::summarize_prompt_with_budget(&text, &request.options, client.config().chunk_chars());

    emit_progress(&app, &request.job_id, "generate", 0, 1);
    let mut on_delta = |delta: &str| emit_chunk(&app, &request.job_id, delta);
    let mut on_reasoning = |delta: &str| emit_reasoning(&app, &request.job_id, delta);
    let mut on_progress = |stage: &str, current: usize, total: usize| {
        emit_progress(&app, &request.job_id, stage, current as u64, total as u64)
    };
    let result = aicore::run_plan(
        &client,
        &plan,
        &cancel,
        &mut on_progress,
        &mut on_delta,
        &mut on_reasoning,
    )
    .await;
    let summary = result.map_err(ai_error)?;
    job_guard.succeed();

    Ok(AiTextResult {
        text: summary,
        pages: extracted.len() as u32,
        characters: text.len() as u64,
        model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

// ---------------------------------------------------------------------------
// Translate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTranslateRequest {
    pub path: String,
    pub options: TranslateOptions,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

#[tauri::command]
pub async fn ai_translate(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AiTranslateRequest,
) -> Result<AiTextResult, PdfError> {
    let started = std::time::Instant::now();
    let (cancel, mut job_guard) = registry.register_ai_managed(&request.job_id);
    let config = build_config(&app)?;
    let model = config.model.clone();
    let path = crate::paths::input_file(&request.path)?;
    let extracted = extract_pages(
        &app,
        &request.job_id,
        path.as_path(),
        request.password.as_deref(),
        request.pages.as_deref(),
        &cancel,
    )?;
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let total = extracted.len();
    let mut output = String::new();
    let mut characters = 0u64;

    for (index, (page, text)) in extracted.iter().enumerate() {
        if cancel.is_cancelled() {
            // The guard marks the (already Cancelled) record terminal on drop.
            return Err(ai_error(AiError::Cancelled));
        }
        emit_progress(&app, &request.job_id, "translate", index as u64, total as u64);
        let header = format!("## Page {page}\n\n");
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&header);
        let messages = prompts::translate_page_prompt(&format!("Page {page}"), text, &request.options);
        let mut on_delta = |delta: &str| emit_chunk(&app, &request.job_id, delta);
        let mut on_reasoning = |delta: &str| emit_reasoning(&app, &request.job_id, delta);
        let translated = client
            .chat_stream(&messages, ChatOptions::default(), &cancel, &mut on_delta, &mut on_reasoning)
            .await
            .map_err(ai_error)?;
        output.push_str(translated.trim());
        characters += translated.chars().count() as u64;
    }
    emit_progress(&app, &request.job_id, "translate", total as u64, total as u64);
    job_guard.succeed();
    Ok(AiTextResult {
        text: output,
        pages: total as u32,
        characters,
        model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

// ---------------------------------------------------------------------------
// Ask the document / cleanup / metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAskRequest {
    pub path: String,
    pub question: String,
    pub password: Option<String>,
    pub job_id: String,
}

#[tauri::command]
pub async fn ai_ask(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AiAskRequest,
) -> Result<AiTextResult, PdfError> {
    let started = std::time::Instant::now();
    // Register first so the guard marks a validation failure terminal too.
    let (cancel, mut job_guard) = registry.register_ai_managed(&request.job_id);
    if request.question.trim().is_empty() {
        return Err(PdfError::InvalidInput("Enter a question.".into()));
    }
    let config = build_config(&app)?;
    let model = config.model.clone();
    let path = crate::paths::input_file(&request.path)?;
    let extracted = extract_pages(
        &app,
        &request.job_id,
        path.as_path(),
        request.password.as_deref(),
        None,
        &cancel,
    )?;
    let selected = prompts::select_relevant_pages(&extracted, &request.question, config.chunk_chars());
    let context = selected
        .iter()
        .map(|(page, text)| format!("[page {page}]\n{text}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let messages = prompts::ask_prompt(&context, &request.question);
    emit_progress(&app, &request.job_id, "generate", 0, 1);
    let mut on_delta = |delta: &str| emit_chunk(&app, &request.job_id, delta);
    let mut on_reasoning = |delta: &str| emit_reasoning(&app, &request.job_id, delta);
    let answer = client
        .chat_stream(&messages, ChatOptions::default(), &cancel, &mut on_delta, &mut on_reasoning)
        .await
        .map_err(ai_error)?;
    job_guard.succeed();
    Ok(AiTextResult {
        text: answer,
        pages: selected.len() as u32,
        characters: context.len() as u64,
        model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCleanupRequest {
    pub path: String,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

#[tauri::command]
pub async fn ai_cleanup_text(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AiCleanupRequest,
) -> Result<AiTextResult, PdfError> {
    let started = std::time::Instant::now();
    let (cancel, mut job_guard) = registry.register_ai_managed(&request.job_id);
    let config = build_config(&app)?;
    let model = config.model.clone();
    let path = crate::paths::input_file(&request.path)?;
    let extracted = extract_pages(
        &app,
        &request.job_id,
        path.as_path(),
        request.password.as_deref(),
        request.pages.as_deref(),
        &cancel,
    )?;
    let text = join_text(&extracted);
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let chunks = prompts::chunk_text(&text, client.config().chunk_chars());
    let mut output = String::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(ai_error(AiError::Cancelled));
        }
        emit_progress(&app, &request.job_id, "cleanup", index as u64, chunks.len() as u64);
        let messages = prompts::cleanup_prompt(chunk);
        let cleaned = client
            .chat(&messages, ChatOptions { temperature: Some(0.0), max_tokens: Some(4096) })
            .await
            .map_err(ai_error)?;
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(cleaned.trim());
    }
    job_guard.succeed();
    Ok(AiTextResult {
        text: output.clone(),
        pages: extracted.len() as u32,
        characters: output.chars().count() as u64,
        model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMetadataRequest {
    pub path: String,
    pub password: Option<String>,
    pub job_id: String,
}

#[tauri::command]
pub async fn ai_suggest_metadata(
    app: AppHandle,
    registry: State<'_, JobRegistry>,
    request: AiMetadataRequest,
) -> Result<prompts::MetadataSuggestion, PdfError> {
    let (cancel, mut job_guard) = registry.register_ai_managed(&request.job_id);
    let config = build_config(&app)?;
    let path = crate::paths::input_file(&request.path)?;
    let extracted = extract_pages(
        &app,
        &request.job_id,
        path.as_path(),
        request.password.as_deref(),
        None,
        &cancel,
    )?;
    let text = join_text(&extracted);
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let messages: Vec<ChatMessage> = prompts::metadata_prompt(&text);
    emit_progress(&app, &request.job_id, "metadata", 0, 1);
    let reply = client
        .chat(
            &messages,
            ChatOptions {
                temperature: Some(0.0),
                max_tokens: Some(512),
            },
        )
        .await
        .map_err(ai_error)?;
    let suggestion = prompts::parse_metadata_reply(&reply)
        .ok_or_else(|| PdfError::coded(ErrorCode::AiInvalidResponse, "The model did not return metadata."))?;
    job_guard.succeed();
    Ok(suggestion)
}

#[tauri::command]
pub fn ai_cancel(registry: State<'_, JobRegistry>, job_id: String) {
    registry.cancel(&job_id);
}

/// Saves an AI result as a text/Markdown file (with the usual overwrite rules).
#[tauri::command]
pub fn ai_save_output(
    path: String,
    text: String,
    overwrite: Option<String>,
) -> Result<String, PdfError> {
    let policy = match overwrite.as_deref() {
        Some("replace") => pdfcore::docutil::OverwritePolicy::Replace,
        Some("unique_name") => pdfcore::docutil::OverwritePolicy::UniqueName,
        _ => pdfcore::docutil::OverwritePolicy::Error,
    };
    let path = crate::paths::output_file(&path)?;
    let target = pdfcore::docutil::resolve_output_path(path.as_path(), policy)?;
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(PdfError::from_io)?;
        }
    }
    pdfcore::docutil::write_bytes_atomic(&target, text.as_bytes())?;
    Ok(target.display().to_string())
}

/// Model ids offered in the settings dropdown (id + human label).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelOption {
    pub id: String,
    pub label: String,
    pub recommended: bool,
}

/// Static fallback list (used before a provider answers, or when discovery is
/// unavailable). The live list comes from [`ai_discover_models`].
#[tauri::command]
pub fn ai_models() -> Vec<AiModelOption> {
    aicore::SUGGESTED_MODELS
        .iter()
        .map(|(id, label)| AiModelOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
            recommended: *id == aicore::DEFAULT_MODEL,
        })
        .collect()
}

/// Result of a live model lookup. `models` is empty when the provider does not
/// expose a listing (the UI then keeps its fallback list). `message` explains
/// what happened without ever containing a key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiDiscoveredModels {
    pub models: Vec<AiModelOption>,
    pub discovered: bool,
    pub message: String,
}

/// Asks the configured provider which models it currently serves. Best-effort:
/// a provider that does not answer returns `discovered: false` with an empty
/// list and the settings screen keeps its static suggestions. This removes the
/// hard-coded assumption that only the models in `SUGGESTED_MODELS` exist.
#[tauri::command]
pub async fn ai_discover_models(app: AppHandle) -> Result<AiDiscoveredModels, PdfError> {
    let config = build_config(&app)?;
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    match client.discover_models().await {
        Ok(models) if !models.is_empty() => Ok(AiDiscoveredModels {
            models: models
                .into_iter()
                .map(|model| AiModelOption {
                    recommended: false,
                    id: model.id,
                    label: model.label.unwrap_or_default(),
                })
                .collect(),
            discovered: true,
            message: String::new(),
        }),
        Ok(_) => Ok(AiDiscoveredModels {
            models: Vec::new(),
            discovered: false,
            message: "The provider did not list any models; using the built-in suggestions.".to_string(),
        }),
        Err(error) => Ok(AiDiscoveredModels {
            models: Vec::new(),
            discovered: false,
            message: ai_error(error).to_string(),
        }),
    }
}

/// Also referenced by the UI to know if AI features are worth showing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExamplePrompts {
    pub summarize: Vec<String>,
    pub ask: Vec<String>,
    pub translate_targets: Vec<String>,
}

#[tauri::command]
pub fn ai_example_prompts() -> AiExamplePrompts {
    AiExamplePrompts {
        summarize: vec![
            "Summarize this document".into(),
            "List the action items and deadlines".into(),
            "Extract all amounts and dates".into(),
        ],
        ask: vec![
            "What is the total amount?".into(),
            "Who signed this document?".into(),
            "What are the payment terms?".into(),
        ],
        translate_targets: vec![
            "tr".into(),
            "en".into(),
            "de".into(),
            "fr".into(),
            "es".into(),
            "ar".into(),
            "ru".into(),
        ],
    }
}

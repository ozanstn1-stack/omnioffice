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
use officecore::model::{Deck, TextDocument, Workbook};
use officecore::{csvio, docx, legacy, odf, pptx, rtf, textio, xlsx};
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
        AiError::InvalidResponse(_) | AiError::Truncated(_) => ErrorCode::AiInvalidResponse,
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
    /// Key state of every provider (keys are stored per provider), so the
    /// settings can show the right one when another provider is selected.
    pub provider_keys: std::collections::BTreeMap<String, AiKeyState>,
}

/// Whether a key is stored for one provider, never the key itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyState {
    pub configured: bool,
    pub masked_key: String,
    /// "dpapi" | "keystore" | "plain" | "none".
    pub key_storage: String,
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
    let dir =
        app.path().app_config_dir().map_err(|error| PdfError::Internal(format!("config dir unavailable: {error}")))?;
    std::fs::create_dir_all(&dir).map_err(PdfError::from_io)?;
    Ok(dir)
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, PdfError> {
    Ok(config_dir(app)?.join("ai.json"))
}

/// Key file of the single-key versions; moved to the saved provider's file on
/// first use.
const LEGACY_KEY_FILE: &str = "ai-key.bin";

const ALL_PROVIDERS: [aicore::ProviderKind; 6] = [
    aicore::ProviderKind::DeepSeek,
    aicore::ProviderKind::OpenAiCompatible,
    aicore::ProviderKind::Ollama,
    aicore::ProviderKind::Gemini,
    aicore::ProviderKind::Anthropic,
    aicore::ProviderKind::Custom,
];

/// Every provider has its own key file, so a key typed for one provider is
/// never sent to another: `ai-key-<provider>.bin`.
fn provider_key_path_in(dir: &Path, kind: aicore::ProviderKind) -> PathBuf {
    dir.join(format!("ai-key-{}.bin", kind.as_str()))
}

/// Moves the legacy `ai-key.bin` to `saved`'s file (the provider the old key
/// was used with). A rename keeps the stored blob byte for byte, so DPAPI /
/// Keystore protection carries over and the key is never lost: an existing
/// per-provider file is not overwritten (the legacy file stays).
fn migrate_legacy_key_in(dir: &Path, saved: aicore::ProviderKind) -> Result<(), PdfError> {
    let legacy = dir.join(LEGACY_KEY_FILE);
    if !legacy.exists() {
        return Ok(());
    }
    let target = provider_key_path_in(dir, saved);
    if target.exists() {
        return Ok(());
    }
    match std::fs::rename(&legacy, &target) {
        Ok(()) => Ok(()),
        // Another command migrated it first.
        Err(_) if !legacy.exists() => Ok(()),
        Err(error) => Err(PdfError::from_io(error)),
    }
}

fn migrate_legacy_key(app: &AppHandle) -> Result<(), PdfError> {
    migrate_legacy_key_in(&config_dir(app)?, provider_kind(&load_settings_file(app).provider))
}

/// Key file for a provider (after migrating the legacy single key).
fn key_path(app: &AppHandle, kind: aicore::ProviderKind) -> Result<PathBuf, PdfError> {
    migrate_legacy_key(app)?;
    Ok(provider_key_path_in(&config_dir(app)?, kind))
}

fn key_state(path: &Path, kind: aicore::ProviderKind) -> AiKeyState {
    let key = secret::load_api_key(path).unwrap_or_default();
    AiKeyState {
        // A local Ollama server needs no API key, so it counts as configured.
        configured: !key.trim().is_empty() || kind == aicore::ProviderKind::Ollama,
        masked_key: mask_key(&key),
        key_storage: secret::storage_kind(path).to_string(),
    }
}

/// The base URL to store: the entered one, or the selected provider's own
/// default (never another provider's) when the field is empty.
fn resolve_base_url(kind: aicore::ProviderKind, entered: &str) -> Result<String, AiError> {
    let trimmed = entered.trim();
    aicore::normalize_base_url(if trimmed.is_empty() { kind.default_base_url() } else { trimmed })
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
        return if trimmed.is_empty() { String::new() } else { "••••".into() };
    }
    format!("{}••••{}", &trimmed[..4], &trimmed[trimmed.len() - 4..])
}

fn view(app: &AppHandle) -> AiSettingsView {
    let settings = load_settings_file(app);
    let provider = provider_kind(&settings.provider);
    let provider_keys: std::collections::BTreeMap<String, AiKeyState> = ALL_PROVIDERS
        .iter()
        .map(|kind| {
            let state = match key_path(app, *kind) {
                Ok(path) => key_state(&path, *kind),
                Err(_) => AiKeyState {
                    configured: *kind == aicore::ProviderKind::Ollama,
                    masked_key: String::new(),
                    key_storage: "none".to_string(),
                },
            };
            (kind.as_str().to_string(), state)
        })
        .collect();
    let current = provider_keys.get(provider.as_str()).cloned().unwrap_or(AiKeyState {
        configured: false,
        masked_key: String::new(),
        key_storage: "none".to_string(),
    });
    AiSettingsView {
        configured: current.configured,
        key_storage: current.key_storage,
        masked_key: current.masked_key,
        provider_keys,
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
    let kind = provider_kind(&settings.provider);
    // A stored key that cannot be decrypted (lost Keystore key) is reported as
    // such instead of as a missing key, so the user knows to enter it again.
    let (key, key_error) = match key_path(app, kind).and_then(|path| secret::load_api_key(&path)) {
        Ok(key) => (key, None),
        Err(error) => (String::new(), Some(error)),
    };
    let config = AiConfig {
        api_key: key,
        base_url: settings.base_url,
        model: settings.model,
        temperature: settings.temperature,
        max_tokens: aicore::clamp_output_tokens(settings.max_tokens),
        thinking: settings.thinking,
        reasoning_effort: settings.reasoning_effort,
        context_tokens: settings.context_tokens.clamp(8_000, aicore::MAX_CONTEXT_TOKENS),
        provider: kind,
        embedding_model: settings.embedding_model.clone(),
    };
    if !config.is_configured() {
        return Err(key_error.unwrap_or_else(|| ai_error(AiError::MissingApiKey)));
    }
    Ok(config)
}

#[tauri::command]
pub fn ai_get_settings(app: AppHandle) -> AiSettingsView {
    view(&app)
}

#[tauri::command]
pub fn ai_save_settings(app: AppHandle, input: AiSettingsInput) -> Result<AiSettingsView, PdfError> {
    // The legacy single key belongs to the provider saved so far, so it moves
    // before the settings (and with them the provider) change.
    migrate_legacy_key(&app)?;
    let kind = input.provider.as_deref().map(provider_kind).unwrap_or_default();
    let file = AiSettingsFile {
        base_url: resolve_base_url(kind, &input.base_url).map_err(ai_error)?,
        model: if input.model.trim().is_empty() {
            kind.default_model().unwrap_or(aicore::DEFAULT_MODEL).to_string()
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
                "medium" => "medium".to_string(),
                "xhigh" => "xhigh".to_string(),
                "max" => "max".to_string(),
                _ => "high".to_string(),
            })
            .unwrap_or_else(default_reasoning_effort),
        context_tokens: input
            .context_tokens
            .unwrap_or_else(default_context_tokens)
            .clamp(8_000, aicore::MAX_CONTEXT_TOKENS),
        provider: kind.as_str().to_string(),
        embedding_model: input.embedding_model.filter(|value| !value.trim().is_empty()),
    };
    let text = serde_json::to_string_pretty(&file)
        .map_err(|error| PdfError::Internal(format!("settings serialize failed: {error}")))?;
    crate::commands::write_atomic(&settings_path(&app)?, text.as_bytes())?;
    if let Some(key) = input.api_key {
        if !key.trim().is_empty() {
            secret::save_api_key(&key_path(&app, kind)?, &key)?;
        }
    }
    Ok(view(&app))
}

#[tauri::command]
pub fn ai_clear_key(app: AppHandle, provider: Option<String>) -> Result<AiSettingsView, PdfError> {
    // Clears the key of the provider the settings screen shows (the saved one
    // when none is named), never another provider's.
    let kind = match provider {
        Some(value) => provider_kind(&value),
        None => provider_kind(&load_settings_file(&app).provider),
    };
    secret::delete_api_key(&key_path(&app, kind)?)?;
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
        Ok(reply) => Ok(AiTestResult { ok: true, message: reply, model }),
        Err(error) => Ok(AiTestResult { ok: false, message: ai_error(error).to_string(), model }),
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
    let _ = app.emit("ai:chunk", serde_json::json!({ "jobId": job_id, "delta": delta, "kind": "content" }));
}

/// Thinking-mode deltas are streamed separately so the UI can show them as a
/// dimmed trace instead of mixing them into the answer.
fn emit_reasoning(app: &AppHandle, job_id: &str, delta: &str) {
    let _ = app.emit("ai:chunk", serde_json::json!({ "jobId": job_id, "delta": delta, "kind": "reasoning" }));
}

/// Maps an office engine error to the app error type without losing the
/// engine's own user-facing message.
fn office_error(error: officecore::OfficeError) -> PdfError {
    let code = match error.code.as_str() {
        "unsupported_format" | "unsupported_feature" => ErrorCode::Unsupported,
        "not_found" => ErrorCode::NotFound,
        "permission_denied" => ErrorCode::PermissionDenied,
        "too_large" => ErrorCode::AiTooLarge,
        "cancelled" => ErrorCode::Cancelled,
        "corrupt_document" | "zip_bomb" | "encoding_error" => ErrorCode::CorruptPdf,
        "invalid_argument" | "invalid_path" => ErrorCode::InvalidInput,
        _ => ErrorCode::Internal,
    };
    PdfError::coded(code, error.message)
}

/// Content units of a Writer document: chunks of at most 4,000 characters so
/// one long document does not become a single prompt page.
fn units_from_document(document: &TextDocument) -> Vec<(u32, String)> {
    let mut units: Vec<(u32, String)> = Vec::new();
    let mut current = String::new();
    for block in &document.blocks {
        let text = block.plain_text();
        if text.trim().is_empty() {
            continue;
        }
        if current.len() + text.len() > 4_000 && !current.is_empty() {
            units.push((units.len() as u32 + 1, std::mem::take(&mut current)));
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(&text);
    }
    if !current.trim().is_empty() {
        units.push((units.len() as u32 + 1, current));
    }
    units
}

/// One unit per sheet, with cell addresses so the assistant can talk about
/// specific cells.
fn units_from_workbook(workbook: &Workbook) -> Vec<(u32, String)> {
    let mut units = Vec::new();
    for (index, sheet) in workbook.sheets.iter().enumerate() {
        let mut text = format!("Sheet: {}\n", sheet.name);
        for (address, cell) in &sheet.cells {
            let value = match &cell.value {
                officecore::model::CellValue::Text(value) => value.clone(),
                officecore::model::CellValue::Number(value) => value.to_string(),
                officecore::model::CellValue::Bool(value) => value.to_string(),
                officecore::model::CellValue::Error(value) => value.clone(),
                officecore::model::CellValue::Empty => {
                    if cell.formula.is_none() {
                        continue;
                    }
                    String::new()
                }
            };
            match &cell.formula {
                Some(formula) => text.push_str(&format!("{address}: {formula} = {value}\n")),
                None => text.push_str(&format!("{address}: {value}\n")),
            }
        }
        if text.lines().count() > 1 {
            units.push((index as u32 + 1, text));
        }
    }
    units
}

/// One unit per slide (shape text, group children and speaker notes).
fn units_from_deck(deck: &Deck) -> Vec<(u32, String)> {
    fn object_text(object: &officecore::model::SlideObject, out: &mut String) {
        if let Some(frame) = &object.text {
            let plain = frame.plain();
            if !plain.trim().is_empty() {
                out.push_str(&plain);
                out.push('\n');
            }
        }
        for child in &object.children {
            object_text(child, out);
        }
    }
    let mut units = Vec::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        let mut text = String::new();
        for object in &slide.objects {
            object_text(object, &mut text);
        }
        if !slide.notes.trim().is_empty() {
            text.push_str("\nNotes: ");
            text.push_str(&slide.notes);
            text.push('\n');
        }
        if !text.trim().is_empty() {
            units.push((index as u32 + 1, text));
        }
    }
    units
}

/// Text extraction for every non-PDF format the suite opens: office documents
/// (including legacy `.doc`/`.ppt` and the native `.oswk` unit), spreadsheets
/// and presentations. The returned "page" numbers are logical units - content
/// chunks, sheets or slides - so the existing page selector still works.
fn extract_office_pages(
    path: &Path,
    pages: Option<&[u32]>,
    cancel: &CancelToken,
) -> Result<Vec<(u32, String)>, PdfError> {
    let extension = path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let units: Vec<(u32, String)> = match extension.as_str() {
        "docx" | "docm" | "dotx" => units_from_document(&docx::read_docx_file(path).map_err(office_error)?.document),
        "odt" => units_from_document(&odf::read_odt_file(path).map_err(office_error)?.document),
        "rtf" => units_from_document(&rtf::read_rtf_file(path).map_err(office_error)?.document),
        "doc" | "dot" => units_from_document(&legacy::read_doc_file(path).map_err(office_error)?.document),
        "txt" | "md" | "markdown" | "html" | "htm" => {
            let bytes = std::fs::read(path).map_err(PdfError::from_io)?;
            let text = officecore::zip::decode_utf8(&bytes, "text").map_err(office_error)?;
            units_from_document(&textio::text_to_document(&text, &officecore::io::file_stem(path)))
        }
        "xlsx" | "xlsm" | "xls" | "ods" => {
            units_from_workbook(&xlsx::read_workbook_file(path).map_err(office_error)?.workbook)
        }
        "csv" | "tsv" => {
            let bytes = std::fs::read(path).map_err(PdfError::from_io)?;
            let mut options = csvio::CsvOptions::default();
            if extension == "tsv" {
                options.delimiter = "tab".into();
            }
            units_from_workbook(&csvio::parse_csv(&bytes, &options).map_err(office_error)?.workbook)
        }
        "pptx" | "pptm" => units_from_deck(&pptx::read_pptx_file(path).map_err(office_error)?.deck),
        "odp" => units_from_deck(&odf::read_odp_file(path).map_err(office_error)?.deck),
        "ppt" => units_from_deck(&legacy::read_ppt_file(path).map_err(office_error)?.document),
        "oswk" => {
            let bytes = std::fs::read(path).map_err(PdfError::from_io)?;
            let mut raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
                PdfError::coded(ErrorCode::CorruptPdf, format!("The unit file is not valid: {error}"))
            })?;
            officecore::unit::verify_checksum(&raw).map_err(office_error)?;
            officecore::schema::migrate_unit(&mut raw).map_err(office_error)?;
            let kind = raw.get("kind").and_then(|value| value.as_str()).unwrap_or_default().to_string();
            let model = raw.get("model").cloned().unwrap_or(serde_json::Value::Null);
            let json_error = |error: serde_json::Error| {
                PdfError::coded(ErrorCode::CorruptPdf, format!("The unit model could not be read: {error}"))
            };
            match kind.as_str() {
                "writer" => units_from_document(&serde_json::from_value(model).map_err(json_error)?),
                "calc" => units_from_workbook(&serde_json::from_value(model).map_err(json_error)?),
                "impress" => units_from_deck(&serde_json::from_value(model).map_err(json_error)?),
                other => {
                    return Err(PdfError::coded(
                        ErrorCode::Unsupported,
                        format!("The AI assistant cannot read .oswk units of kind {other}."),
                    ));
                }
            }
        }
        other => {
            return Err(PdfError::coded(
                ErrorCode::Unsupported,
                format!("The AI assistant does not support .{other} files."),
            ));
        }
    };

    let cleaned: Vec<(u32, String)> = units
        .into_iter()
        .filter_map(|(number, text)| {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                None
            } else {
                Some((number, text))
            }
        })
        .collect();
    if cleaned.is_empty() {
        return Err(ai_error(AiError::NoText));
    }
    let selected: Vec<(u32, String)> = match pages {
        Some(list) if !list.is_empty() => {
            let mut out = Vec::with_capacity(list.len());
            for page in list {
                match cleaned.iter().find(|(number, _)| number == page) {
                    Some(entry) => out.push(entry.clone()),
                    None => return Err(PdfError::RangeOutOfBounds),
                }
            }
            out
        }
        _ => cleaned.into_iter().take(MAX_PAGES_FOR_CONTEXT as usize).collect(),
    };
    if cancel.is_cancelled() {
        return Err(ai_error(AiError::Cancelled));
    }
    let characters: usize = selected.iter().map(|(_, text)| text.len()).sum();
    if characters < MIN_TEXT_CHARS {
        return Err(ai_error(AiError::NoText));
    }
    Ok(selected)
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
    let extension = path.extension().map(|value| value.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if extension != "pdf" {
        return extract_office_pages(path, pages, cancel);
    }
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
    let extracted = extract_pages(&app, &job_id, path.as_path(), password.as_deref(), pages.as_deref(), &cancel)?;
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSummarizeRequest {
    pub path: String,
    pub options: SummaryOptions,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

/// Manual `Debug`: the derived one would print the document password into any
/// log line or error context that formats the request.
impl std::fmt::Debug for AiSummarizeRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSummarizeRequest")
            .field("path", &self.path)
            .field("options", &self.options)
            .field("pages", &self.pages)
            .field("password", &self.password.as_ref().map(|_| "**REDACTED**"))
            .field("job_id", &self.job_id)
            .finish()
    }
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
    let result = aicore::run_plan(&client, &plan, &cancel, &mut on_progress, &mut on_delta, &mut on_reasoning).await;
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTranslateRequest {
    pub path: String,
    pub options: TranslateOptions,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

/// Manual `Debug`; see [`AiSummarizeRequest`] for why the password is redacted.
impl std::fmt::Debug for AiTranslateRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiTranslateRequest")
            .field("path", &self.path)
            .field("options", &self.options)
            .field("pages", &self.pages)
            .field("password", &self.password.as_ref().map(|_| "**REDACTED**"))
            .field("job_id", &self.job_id)
            .finish()
    }
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAskRequest {
    pub path: String,
    pub question: String,
    pub password: Option<String>,
    pub job_id: String,
}

/// Manual `Debug`; see [`AiSummarizeRequest`] for why the password is redacted.
impl std::fmt::Debug for AiAskRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiAskRequest")
            .field("path", &self.path)
            .field("question", &self.question)
            .field("password", &self.password.as_ref().map(|_| "**REDACTED**"))
            .field("job_id", &self.job_id)
            .finish()
    }
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
    let extracted = extract_pages(&app, &request.job_id, path.as_path(), request.password.as_deref(), None, &cancel)?;
    let selected = prompts::select_relevant_pages(&extracted, &request.question, config.chunk_chars());
    let context = selected.iter().map(|(page, text)| format!("[page {page}]\n{text}")).collect::<Vec<_>>().join("\n\n");
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCleanupRequest {
    pub path: String,
    pub pages: Option<Vec<u32>>,
    pub password: Option<String>,
    pub job_id: String,
}

/// Manual `Debug`; see [`AiSummarizeRequest`] for why the password is redacted.
impl std::fmt::Debug for AiCleanupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiCleanupRequest")
            .field("path", &self.path)
            .field("pages", &self.pages)
            .field("password", &self.password.as_ref().map(|_| "**REDACTED**"))
            .field("job_id", &self.job_id)
            .finish()
    }
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

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMetadataRequest {
    pub path: String,
    pub password: Option<String>,
    pub job_id: String,
}

/// Manual `Debug`; see [`AiSummarizeRequest`] for why the password is redacted.
impl std::fmt::Debug for AiMetadataRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiMetadataRequest")
            .field("path", &self.path)
            .field("password", &self.password.as_ref().map(|_| "**REDACTED**"))
            .field("job_id", &self.job_id)
            .finish()
    }
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
    let extracted = extract_pages(&app, &request.job_id, path.as_path(), request.password.as_deref(), None, &cancel)?;
    let text = join_text(&extracted);
    let client = DeepSeekClient::new(config).map_err(ai_error)?;
    let messages: Vec<ChatMessage> = prompts::metadata_prompt(&text);
    emit_progress(&app, &request.job_id, "metadata", 0, 1);
    let reply = client
        .chat(&messages, ChatOptions { temperature: Some(0.0), max_tokens: Some(512) })
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
pub fn ai_save_output(path: String, text: String, overwrite: Option<String>) -> Result<String, PdfError> {
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
pub fn ai_models(provider: Option<String>) -> Vec<AiModelOption> {
    let kind = provider.as_deref().map(provider_kind).unwrap_or_default();
    let default_model = match kind {
        aicore::ProviderKind::Anthropic => aicore::ANTHROPIC_DEFAULT_MODEL,
        _ => aicore::DEFAULT_MODEL,
    };
    aicore::suggested_models(kind)
        .iter()
        .map(|(id, label)| AiModelOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
            recommended: *id == default_model,
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
                .map(|model| AiModelOption { recommended: false, id: model.id, label: model.label.unwrap_or_default() })
                .collect(),
            discovered: true,
            message: String::new(),
        }),
        Ok(_) => Ok(AiDiscoveredModels {
            models: Vec::new(),
            discovered: false,
            message: "The provider did not list any models; using the built-in suggestions.".to_string(),
        }),
        Err(error) => {
            Ok(AiDiscoveredModels { models: Vec::new(), discovered: false, message: ai_error(error).to_string() })
        }
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

pub mod edit;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every request type that carries a document password must redact it in
    /// its `Debug` output; a derived `Debug` would print it into any log line
    /// or error context.
    #[test]
    fn request_debug_redacts_document_passwords() {
        let password = "correct horse battery staple";
        let summarize = AiSummarizeRequest {
            path: "C:/docs/report.pdf".into(),
            options: SummaryOptions::default(),
            pages: Some(vec![1, 2]),
            password: Some(password.into()),
            job_id: "job-1".into(),
        };
        let translate = AiTranslateRequest {
            path: "C:/docs/report.pdf".into(),
            options: TranslateOptions { target_language: "tr".into(), bilingual: true },
            pages: None,
            password: Some(password.into()),
            job_id: "job-2".into(),
        };
        let ask = AiAskRequest {
            path: "C:/docs/report.pdf".into(),
            question: "total?".into(),
            password: Some(password.into()),
            job_id: "job-3".into(),
        };
        let cleanup = AiCleanupRequest {
            path: "C:/docs/report.pdf".into(),
            pages: None,
            password: Some(password.into()),
            job_id: "job-4".into(),
        };
        let metadata = AiMetadataRequest {
            path: "C:/docs/report.pdf".into(),
            password: Some(password.into()),
            job_id: "job-5".into(),
        };

        for formatted in [
            format!("{summarize:?}"),
            format!("{translate:?}"),
            format!("{ask:?}"),
            format!("{cleanup:?}"),
            format!("{metadata:?}"),
        ] {
            assert!(!formatted.contains(password), "password leaked: {formatted}");
            assert!(formatted.contains("REDACTED"), "redaction marker missing: {formatted}");
        }
    }

    #[test]
    fn request_debug_without_a_password_is_clean() {
        let request = AiMetadataRequest { path: "C:/docs/a.pdf".into(), password: None, job_id: "job-6".into() };
        let formatted = format!("{request:?}");
        assert!(!formatted.contains("REDACTED"), "no marker when there is nothing to redact: {formatted}");
    }

    /// The AI assistant reads PDFs plus every office format the suite opens:
    /// DOCX/ODT/RTF/legacy DOC as text chunks, XLSX/ODS/CSV as one unit per
    /// sheet and PPTX/ODP/legacy PPT as one unit per slide.
    #[test]
    fn office_documents_are_extracted_for_the_assistant() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cancel = CancelToken::new();

        let mut document = officecore::model::TextDocument::new_blank("AI test");
        document.blocks = vec![officecore::model::Block::paragraph("Merhaba d�nya bu bir deneme metnidir.")];
        let docx = dir.path().join("ai.docx");
        officecore::docx::write_docx_file(&docx, &document).expect("write docx");
        let units = extract_office_pages(&docx, None, &cancel).expect("docx units");
        assert!(units.iter().any(|(_, text)| text.contains("Merhaba d�nya")), "{units:?}");

        let mut workbook = officecore::model::Workbook::new_blank("AI test");
        workbook.sheets[0].set(
            "A1",
            officecore::model::Cell { value: officecore::model::CellValue::Text("Gelir".into()), ..Default::default() },
        );
        workbook.sheets[0].set(
            "B1",
            officecore::model::Cell { value: officecore::model::CellValue::Number(42.0), ..Default::default() },
        );
        workbook.sheets[0].set(
            "A2",
            officecore::model::Cell {
                value: officecore::model::CellValue::Text("Toplam".into()),
                ..Default::default()
            },
        );
        let xlsx = dir.path().join("ai.xlsx");
        officecore::xlsx::write_xlsx_file(&xlsx, &workbook).expect("write xlsx");
        let units = extract_office_pages(&xlsx, None, &cancel).expect("xlsx units");
        assert!(units[0].1.contains("Gelir") && units[0].1.contains("42"), "{units:?}");

        let mut deck = officecore::model::Deck::new_blank("AI test");
        let mut slide = officecore::model::Slide::default();
        let mut object = officecore::model::SlideObject::new("text", 10.0, 10.0, 200.0, 50.0);
        object.text = Some(officecore::model::TextFrame {
            paragraphs: vec![officecore::model::TextParagraph {
                text: "Sunum metni bu bir deneme slaytıdır.".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        slide.objects.push(object);
        deck.slides = vec![slide];
        let pptx = dir.path().join("ai.pptx");
        officecore::pptx::write_pptx_file(&pptx, &deck).expect("write pptx");
        let units = extract_office_pages(&pptx, None, &cancel).expect("pptx units");
        assert!(units.iter().any(|(_, text)| text.contains("Sunum metni")), "{units:?}");

        let unknown = dir.path().join("ai.zip");
        std::fs::write(&unknown, b"not a document").expect("write zip");
        assert!(extract_office_pages(&unknown, None, &cancel).is_err());
    }
    #[test]
    fn every_provider_has_its_own_key_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut names: Vec<String> = ALL_PROVIDERS
            .iter()
            .map(|kind| provider_key_path_in(dir.path(), *kind).file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"ai-key-anthropic.bin".to_string()), "{names:?}");
        assert!(names.contains(&"ai-key-deepseek.bin".to_string()), "{names:?}");
        names.sort();
        names.dedup();
        assert_eq!(names.len(), ALL_PROVIDERS.len());

        let anthropic = provider_key_path_in(dir.path(), aicore::ProviderKind::Anthropic);
        let deepseek = provider_key_path_in(dir.path(), aicore::ProviderKind::DeepSeek);
        secret::save_api_key(&anthropic, "sk-ant-one").expect("save anthropic");
        // Another provider never sees it.
        assert_eq!(secret::load_api_key(&deepseek).unwrap_or_default(), "");
        assert!(!key_state(&deepseek, aicore::ProviderKind::DeepSeek).configured);
        assert!(key_state(&anthropic, aicore::ProviderKind::Anthropic).configured);
        // Ollama needs no key.
        let ollama = provider_key_path_in(dir.path(), aicore::ProviderKind::Ollama);
        assert!(key_state(&ollama, aicore::ProviderKind::Ollama).configured);
        secret::save_api_key(&deepseek, "sk-ds-two").expect("save deepseek");
        assert_eq!(secret::load_api_key(&anthropic).unwrap(), "sk-ant-one");
        assert_eq!(secret::load_api_key(&deepseek).unwrap(), "sk-ds-two");
    }

    #[test]
    fn the_legacy_key_moves_to_the_saved_provider_without_loss() {
        let dir = tempfile::tempdir().expect("tempdir");
        let legacy = dir.path().join(LEGACY_KEY_FILE);
        secret::save_api_key(&legacy, "sk-ant-legacy").expect("save legacy");
        let raw = std::fs::read(&legacy).unwrap();

        migrate_legacy_key_in(dir.path(), aicore::ProviderKind::Anthropic).expect("migrate");
        assert!(!legacy.exists());
        let anthropic = provider_key_path_in(dir.path(), aicore::ProviderKind::Anthropic);
        // The stored blob is moved byte for byte (DPAPI / Keystore stay valid).
        assert_eq!(std::fs::read(&anthropic).unwrap(), raw);
        assert_eq!(secret::load_api_key(&anthropic).unwrap(), "sk-ant-legacy");
        let deepseek = provider_key_path_in(dir.path(), aicore::ProviderKind::DeepSeek);
        assert!(!deepseek.exists());

        // Idempotent when nothing is left to migrate.
        migrate_legacy_key_in(dir.path(), aicore::ProviderKind::DeepSeek).expect("noop");
        assert!(!deepseek.exists());
    }

    #[test]
    fn migration_never_overwrites_an_existing_provider_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let legacy = dir.path().join(LEGACY_KEY_FILE);
        let target = provider_key_path_in(dir.path(), aicore::ProviderKind::DeepSeek);
        secret::save_api_key(&legacy, "old").expect("save legacy");
        secret::save_api_key(&target, "new").expect("save target");
        migrate_legacy_key_in(dir.path(), aicore::ProviderKind::DeepSeek).expect("migrate");
        assert_eq!(secret::load_api_key(&target).unwrap(), "new");
        // The legacy value is kept rather than dropped.
        assert_eq!(secret::load_api_key(&legacy).unwrap(), "old");
    }

    #[test]
    fn an_empty_base_url_falls_back_to_the_selected_providers_own_default() {
        use aicore::ProviderKind;
        assert_eq!(resolve_base_url(ProviderKind::Anthropic, "  ").unwrap(), aicore::ANTHROPIC_BASE_URL);
        assert_eq!(resolve_base_url(ProviderKind::DeepSeek, "").unwrap(), aicore::DEFAULT_BASE_URL);
        assert_eq!(resolve_base_url(ProviderKind::OpenAiCompatible, "").unwrap(), "https://api.openai.com/v1");
        assert_eq!(resolve_base_url(ProviderKind::Ollama, "").unwrap(), "http://localhost:11434");
        assert!(resolve_base_url(ProviderKind::Gemini, "").unwrap().contains("generativelanguage"));
        // A custom endpoint has no default: the empty value is an error, not DeepSeek.
        assert!(resolve_base_url(ProviderKind::Custom, "").is_err());
        assert_eq!(
            resolve_base_url(ProviderKind::Custom, " https://llm.example/v1/ ").unwrap(),
            "https://llm.example/v1"
        );
    }
}

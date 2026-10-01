//! aicore — DeepSeek API client and prompt pipeline for PDF Swiss Army Knife.
//!
//! This is the **only** component in the project that talks to the network.
//! It is opt-in: nothing is sent unless the user explicitly runs an AI action
//! with their own API key, and the text that is sent is exactly the extracted
//! document text the user chose to process.
//!
//! The crate is independent from the UI and from pdfcore so it can be tested
//! against a local mock server (see `tests/`).

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub mod prompts;

pub use prompts::{chunk_text, summarize_prompt_with_budget, Plan};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-flash";

/// Model ids offered by the DeepSeek platform (chat completions).
/// `deepseek-v4-flash` currently serves DeepSeek-V4-Flash-0731; older aliases
/// are kept for accounts that still use them.
pub const SUGGESTED_MODELS: &[(&str, &str)] = &[
    ("deepseek-flash", "DeepSeek V4.1 Flash (fast, 1M context, recommended)"),
    ("deepseek-v4-flash", "deepseek-v4-flash (legacy alias, served by V4.1 Flash)"),
    ("deepseek-v4-flash-vision-exp", "DeepSeek V4 Flash Vision (experimental, image input)"),
    ("deepseek-v4-pro", "DeepSeek V4 Pro (highest quality, slower)"),
    ("deepseek-chat", "deepseek-chat (legacy alias)"),
    ("deepseek-reasoner", "deepseek-reasoner (legacy reasoning alias)"),
];

/// DeepSeek V4 models share a 1,000,000 token context window (input + output)
/// and cap the generated output at 384,000 tokens.
pub const MAX_CONTEXT_TOKENS: u32 = 1_000_000;
pub const MAX_OUTPUT_TOKENS: u32 = 384_000;
/// Conservative characters-per-token ratio used to turn the token budget into
/// the amount of document text sent per request. Turkish tokenizes denser than
/// English, so this stays well below the usual 4 chars/token.
pub const CHARS_PER_TOKEN: f32 = 2.5;

/// Characters of document text that fit into the configured context budget,
/// leaving room for the instructions and the generated answer.
pub fn chunk_chars_for_context(context_tokens: u32) -> usize {
    let context = context_tokens.clamp(8_000, MAX_CONTEXT_TOKENS);
    // Reserve ~20% of the window for the prompt scaffolding and the answer.
    let budget_tokens = (context as f32 * 0.8) as usize;
    ((budget_tokens as f32 * CHARS_PER_TOKEN) as usize).clamp(8_000, 4_000_000)
}

/// Output tokens clamped to what the API accepts (384K maximum).
pub fn clamp_output_tokens(max_tokens: u32) -> u32 {
    max_tokens.clamp(256, MAX_OUTPUT_TOKENS)
}

/// One model a provider advertises. Fields the provider does not expose are
/// `None`; callers fall back to their defaults rather than guessing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    /// Human label when the provider supplies one (`owned_by` on OpenAI, etc.).
    #[serde(default)]
    pub label: Option<String>,
    /// Context window in tokens, when the provider reports it.
    #[serde(default)]
    pub context_tokens: Option<u32>,
    /// Maximum output tokens, when the provider reports it.
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    /// True when the provider marks the model as free/local.
    #[serde(default)]
    pub local: bool,
}

/// Parses the OpenAI-compatible `GET /models` envelope
/// (`{"data":[{"id":"...","owned_by":"..."}]}`) and, defensively, a bare array.
/// Unknown shapes return an empty list rather than failing: discovery is a
/// convenience, the configured model still works when it returns nothing.
pub fn parse_openai_models(body: &str) -> Vec<ModelInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let entries = value
        .get("data")
        .and_then(|data| data.as_array())
        .or_else(|| value.as_array());
    let Some(entries) = entries else {
        return Vec::new();
    };
    let mut models: Vec<ModelInfo> = entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|id| id.as_str())?.trim();
            if id.is_empty() {
                return None;
            }
            let label = entry
                .get("owned_by")
                .and_then(|owner| owner.as_str())
                .filter(|owner| !owner.trim().is_empty())
                .map(|owner| owner.to_string());
            let context_tokens = entry
                .get("context_length")
                .or_else(|| entry.get("context_window"))
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok());
            Some(ModelInfo {
                id: id.to_string(),
                label,
                context_tokens,
                max_output_tokens: None,
                local: false,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

/// Parses Ollama's native `GET /api/tags` envelope
/// (`{"models":[{"name":"llama3.2:latest","size":...}]}`).
pub fn parse_ollama_models(body: &str) -> Vec<ModelInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let Some(entries) = value.get("models").and_then(|models| models.as_array()) else {
        return Vec::new();
    };
    let mut models: Vec<ModelInfo> = entries
        .iter()
        .filter_map(|entry| {
            let id = entry
                .get("name")
                .or_else(|| entry.get("model"))
                .and_then(|name| name.as_str())?
                .trim();
            if id.is_empty() {
                return None;
            }
            Some(ModelInfo {
                id: id.to_string(),
                label: None,
                context_tokens: None,
                max_output_tokens: None,
                local: true,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    DeepSeek,
    OpenAiCompatible,
    Ollama,
    Gemini,
    Custom,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::OpenAiCompatible => "openai_compatible",
            Self::Ollama => "ollama",
            Self::Gemini => "gemini",
            Self::Custom => "custom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "deepseek" | "deep_seek" => Some(Self::DeepSeek),
            "openai_compatible" | "openai" | "open_ai" | "compatible" => Some(Self::OpenAiCompatible),
            "ollama" | "local" => Some(Self::Ollama),
            "gemini" | "google" | "google_gemini" => Some(Self::Gemini),
            "custom" | "other" => Some(Self::Custom),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => "DeepSeek",
            Self::OpenAiCompatible => "OpenAI-compatible",
            Self::Ollama => "Ollama (local)",
            Self::Gemini => "Google Gemini",
            Self::Custom => "Custom endpoint",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::DeepSeek => DEFAULT_BASE_URL,
            Self::OpenAiCompatible => "https://api.openai.com/v1",
            Self::Ollama => "http://localhost:11434",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            Self::Custom => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub chat: bool,
    pub embeddings: bool,
    pub vision: bool,
    pub structured_output: bool,
    pub streaming: bool,
}

impl ProviderCapabilities {
    pub fn for_kind(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::DeepSeek => Self {
                chat: true,
                embeddings: false,
                vision: true,
                structured_output: true,
                streaming: true,
            },
            ProviderKind::OpenAiCompatible => Self {
                chat: true,
                embeddings: true,
                vision: true,
                structured_output: true,
                streaming: true,
            },
            ProviderKind::Ollama => Self {
                chat: true,
                embeddings: true,
                vision: true,
                structured_output: true,
                streaming: true,
            },
            ProviderKind::Gemini => Self {
                chat: true,
                embeddings: true,
                vision: true,
                structured_output: true,
                streaming: true,
            },
            ProviderKind::Custom => Self {
                chat: true,
                embeddings: false,
                vision: false,
                structured_output: false,
                streaming: true,
            },
        }
    }
}

/// One-line privacy note shown next to the provider picker in the settings.
pub fn provider_notes(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::DeepSeek => {
            "Cloud API. The document text is sent to api.deepseek.com together with your API key."
        }
        ProviderKind::OpenAiCompatible => {
            "Cloud API. The document text is sent to the OpenAI-compatible endpoint you configure."
        }
        ProviderKind::Ollama => {
            "Local models. Ollama runs on your computer and the document text never leaves the computer."
        }
        ProviderKind::Gemini => {
            "Cloud API. The document text is sent to Google Gemini together with your API key."
        }
        ProviderKind::Custom => {
            "Custom endpoint. The document text is sent wherever that endpoint points; check its privacy policy."
        }
    }
}

/// True when `host` points at the local machine: `localhost`, any name under
/// `.localhost`, or a loopback IP literal (`127.0.0.0/8`, `::1`).
pub fn is_loopback_host(host: &str) -> bool {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.eq_ignore_ascii_case("localhost") {
        return true;
    }
    trimmed
        .parse::<std::net::IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

/// Validates and normalizes a provider base URL, trimming trailing slashes.
///
/// The base URL is user-editable and the API key is sent to whatever host it
/// contains as a Bearer token, so HTTPS is mandatory for every non-local host:
/// plain `http://` would put the key and the document text on the wire in
/// cleartext. The loopback exception (Ollama and other local servers) is
/// limited to `localhost` / loopback IPs. Query strings, fragments, embedded
/// credentials and whitespace are rejected because the endpoint is built by
/// appending a path to this value.
pub fn normalize_base_url(url: &str) -> AiResult<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AiError::InvalidBaseUrl("the provider URL is empty".to_string()));
    }
    if trimmed.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err(AiError::InvalidBaseUrl("the provider URL contains whitespace".to_string()));
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| AiError::InvalidBaseUrl("the provider URL is not a valid absolute URL".to_string()))?;
    match parsed.scheme() {
        "https" => {}
        "http" => {
            if !parsed.host_str().map(is_loopback_host).unwrap_or(false) {
                return Err(AiError::InvalidBaseUrl(
                    "plain http:// is refused for a remote provider: the API key and the document text would travel unencrypted. Use https:// (http:// is only allowed for a local server such as Ollama)"
                        .to_string(),
                ));
            }
        }
        other => {
            return Err(AiError::InvalidBaseUrl(format!(
                "the provider URL must start with https:// (got {other}://)"
            )))
        }
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(AiError::InvalidBaseUrl(
            "the provider URL must not contain a query string or fragment".to_string(),
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AiError::InvalidBaseUrl(
            "remove the username/password from the provider URL and use the API key field".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

/// A redirect is only followed over HTTPS, or over plain HTTP when the target
/// is the local machine/the configured loopback server. A redirect to a public
/// `http://` endpoint would leak the Bearer token, so the request fails
/// instead of following it.
fn is_redirect_target_allowed(url: &reqwest::Url, allow_loopback_http: bool) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => allow_loopback_http && url.host_str().map(is_loopback_host).unwrap_or(false),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    pub api_key: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// DeepSeek V4 thinking mode ("thinking": {"type": "enabled"|"disabled"}).
    /// Ignored for models that are not DeepSeek V4.
    #[serde(default = "default_thinking")]
    pub thinking: bool,
    /// "low" | "high" | "max" (DeepSeek maps medium/xhigh to high).
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    /// Input budget in tokens (1M maximum). Controls how much document text is
    /// sent per request and how the map/reduce chunking is sized.
    #[serde(default = "default_context_tokens")]
    pub context_tokens: u32,
    /// Which API backend the client talks to (default: DeepSeek).
    #[serde(default)]
    pub provider: ProviderKind,
    /// Optional embedding model, used by providers that expose embeddings.
    #[serde(default)]
    pub embedding_model: Option<String>,
}

pub fn default_context_tokens() -> u32 {
    200_000
}

fn default_thinking() -> bool {
    true
}

fn default_reasoning_effort() -> String {
    "high".to_string()
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}
fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}
fn default_temperature() -> f32 {
    0.2
}
fn default_max_tokens() -> u32 {
    4096
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_base_url(),
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            thinking: default_thinking(),
            reasoning_effort: default_reasoning_effort(),
            context_tokens: default_context_tokens(),
            provider: ProviderKind::DeepSeek,
            embedding_model: None,
        }
    }
}

impl AiConfig {
    /// Characters of document text per request derived from the context budget.
    pub fn chunk_chars(&self) -> usize {
        chunk_chars_for_context(self.context_tokens)
    }
}

/// DeepSeek V4 models accept the `thinking` and `reasoning_effort` fields;
/// other (legacy or third-party) models would reject or misuse them.
pub fn supports_thinking(model: &str) -> bool {
    let normalized = model.trim().to_lowercase();
    normalized.starts_with("deepseek-v4")
        || normalized == "deepseek-flash"
        || normalized.starts_with("deepseek-flash-")
        || normalized == "deepseek-reasoner"
}

impl AiConfig {
    pub fn is_configured(&self) -> bool {
        self.provider == ProviderKind::Ollama || !self.api_key.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no API key configured")]
    MissingApiKey,
    // The provider name is supplied by the caller so the message is honest for
    // OpenAI-compatible, Gemini and custom endpoints too - not only DeepSeek.
    #[error("the API key was rejected by {0}")]
    InvalidApiKey(String),
    #[error("{0} rate limit reached, try again in a moment")]
    RateLimited(String),
    #[error("the {0} account has insufficient balance")]
    InsufficientBalance(String),
    #[error("could not reach {0}")]
    Network(String),
    #[error("{0} returned an error: {1}")]
    Server(String, String),
    #[error("unexpected response from {0}")]
    InvalidResponse(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("the document has no extractable text (run OCR first)")]
    NoText,
    #[error("request was too large for the model context")]
    TooLarge,
    #[error("{0}")]
    ProviderUnreachable(String),
    #[error("invalid provider URL: {0}")]
    InvalidBaseUrl(String),
    #[error("unsupported action: {0}")]
    Unsupported(String),
}

pub type AiResult<T> = Result<T, AiError>;

/// Cooperative cancellation shared with the UI.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn check(&self) -> AiResult<()> {
        if self.is_cancelled() {
            Err(AiError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Default)]
struct ThinkingSetting {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Serialize)]
struct ChatRequestBody<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingSetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
}

impl<'a> ChatRequestBody<'a> {
    fn new(config: &'a AiConfig, messages: &'a [ChatMessage], options: &ChatOptions, stream: bool) -> Self {
        let enable_thinking = config.provider == ProviderKind::DeepSeek && supports_thinking(&config.model);
        Self {
            model: &config.model,
            messages,
            temperature: options.temperature.unwrap_or(config.temperature),
            max_tokens: clamp_output_tokens(options.max_tokens.unwrap_or(config.max_tokens)),
            stream,
            thinking: if enable_thinking {
                Some(ThinkingSetting {
                    kind: if config.thinking { "enabled" } else { "disabled" }.to_string(),
                })
            } else {
                None
            },
            reasoning_effort: if enable_thinking && config.thinking {
                Some(config.reasoning_effort.trim())
            } else {
                None
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponseBody {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageContent,
}

#[derive(Debug, Deserialize)]
struct ChatMessageContent {
    #[serde(default)]
    content: String,
    /// Present while (and when) the model is in thinking mode.
    #[serde(default)]
    reasoning_content: String,
}

impl ChatMessageContent {
    /// Content when available, otherwise the reasoning trace - a response
    /// that only contains reasoning (for example when max_tokens ran out
    /// while thinking) is still better than an error.
    fn text(&self) -> Option<String> {
        if !self.content.trim().is_empty() {
            return Some(self.content.clone());
        }
        if !self.reasoning_content.trim().is_empty() {
            return Some(self.reasoning_content.clone());
        }
        None
    }
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    #[serde(default)]
    message: String,
    #[serde(default)]
    code: String,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Debug, Serialize)]
struct OllamaChatRequestBody<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    options: OllamaChatOptions,
}

#[derive(Debug, Serialize)]
struct OllamaChatOptions {
    temperature: f32,
    num_predict: u32,
}

impl<'a> OllamaChatRequestBody<'a> {
    fn new(config: &'a AiConfig, messages: &'a [ChatMessage], options: &ChatOptions, stream: bool) -> Self {
        Self {
            model: &config.model,
            messages,
            stream,
            options: OllamaChatOptions {
                temperature: options.temperature.unwrap_or(config.temperature),
                num_predict: clamp_output_tokens(options.max_tokens.unwrap_or(config.max_tokens)),
            },
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct OllamaChatResponse {
    #[serde(default)]
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaMessage {
    #[serde(default)]
    content: String,
}

/// Chat-completions client that adapts to the configured provider.
///
/// The name is kept for backwards compatibility: it talks to DeepSeek by
/// default, and to any OpenAI-compatible, Ollama, Gemini or custom endpoint
/// selected with [`ProviderKind`].
#[derive(Debug)]
pub struct DeepSeekClient {
    config: AiConfig,
    http: reqwest::Client,
}

impl DeepSeekClient {
    pub fn new(mut config: AiConfig) -> AiResult<Self> {
        if !config.is_configured() {
            return Err(AiError::MissingApiKey);
        }
        config.base_url = normalize_base_url(&config.base_url)?;
        let allow_loopback_http = config
            .base_url
            .to_ascii_lowercase()
            .starts_with("http://");
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .connect_timeout(Duration::from_secs(20))
            .user_agent(concat!("PDFSwissArmyKnife/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if !is_redirect_target_allowed(attempt.url(), allow_loopback_http) {
                    return attempt.error(
                        "a redirect to a non-local http:// endpoint was refused to keep the API key encrypted in transit"
                            .to_string(),
                    );
                }
                if attempt.previous().len() >= 5 {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| AiError::ProviderUnreachable("the HTTP client could not be created".to_string()))?;
        Ok(Self { config, http })
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    pub fn provider(&self) -> ProviderKind {
        self.config.provider
    }

    pub fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::for_kind(self.provider())
    }

    /// The provider's display name, used in honest error messages.
    fn provider_name(&self) -> String {
        self.provider().label().to_string()
    }

    /// Endpoint for model discovery. OpenAI-compatible providers list models at
    /// `{base}/models`; Ollama uses its native `{base}/api/tags`.
    fn models_endpoint(&self) -> String {
        let base = self.config.base_url.trim().trim_end_matches('/');
        match self.provider() {
            ProviderKind::Ollama => format!("{base}/api/tags"),
            _ => format!("{base}/models"),
        }
    }

    /// Discovers the models the provider advertises, when it exposes a listing
    /// endpoint. This is best-effort: a provider that does not answer returns an
    /// empty list and the caller keeps its configured model. No API key is ever
    /// logged; on failure only the provider label is named.
    pub async fn discover_models(&self) -> AiResult<Vec<ModelInfo>> {
        let builder = self.http.get(self.models_endpoint());
        let builder = if self.provider() == ProviderKind::Ollama {
            builder
        } else {
            builder.bearer_auth(self.config.api_key.trim())
        };
        let response = builder.send().await.map_err(|error| self.unreachable(error))?;
        let status = response.status();
        let body = response.text().await.map_err(|error| self.unreachable(error))?;
        if !status.is_success() {
            return Err(map_http_error(status.as_u16(), &body, &self.provider_name()));
        }
        Ok(match self.provider() {
            ProviderKind::Ollama => parse_ollama_models(&body),
            _ => parse_openai_models(&body),
        })
    }

    fn endpoint(&self) -> String {
        let base = self.config.base_url.trim().trim_end_matches('/');
        match self.provider() {
            ProviderKind::Ollama => format!("{base}/api/chat"),
            ProviderKind::Gemini => format!("{base}/openai/chat/completions"),
            ProviderKind::DeepSeek | ProviderKind::OpenAiCompatible | ProviderKind::Custom => {
                format!("{base}/chat/completions")
            }
        }
    }

    fn request(&self) -> reqwest::RequestBuilder {
        let builder = self.http.post(self.endpoint());
        if self.provider() == ProviderKind::Ollama {
            builder
        } else {
            builder.bearer_auth(self.config.api_key.trim())
        }
    }

    fn unreachable(&self, error: reqwest::Error) -> AiError {
        let reason = if error.is_timeout() {
            "the request timed out"
        } else {
            "could not reach the server"
        };
        let hint = match self.provider() {
            ProviderKind::Ollama => " (is Ollama running? try `ollama serve`)",
            _ => "",
        };
        AiError::ProviderUnreachable(format!(
            "{reason} for {} at {}{hint}",
            self.provider().label(),
            self.config.base_url.trim()
        ))
    }

    /// Non-streaming completion (used for tests and short tasks).
    pub async fn chat(&self, messages: &[ChatMessage], options: ChatOptions) -> AiResult<String> {
        if self.provider() == ProviderKind::Ollama {
            self.chat_ollama(messages, &options).await
        } else {
            self.chat_openai_compatible(messages, &options).await
        }
    }

    async fn chat_openai_compatible(&self, messages: &[ChatMessage], options: &ChatOptions) -> AiResult<String> {
        let body = ChatRequestBody::new(&self.config, messages, options, false);
        let response = self
            .request()
            .json(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;

        let status = response.status();
        let text = response.text().await.map_err(|error| self.unreachable(error))?;
        if !status.is_success() {
            return Err(map_http_error(status.as_u16(), &text, &self.provider_name()));
        }
        let parsed: ChatResponseBody = serde_json::from_str(&text).map_err(|_| AiError::InvalidResponse(self.provider_name()))?;
        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.text())
            .ok_or_else(|| AiError::InvalidResponse(self.provider_name()))
    }

    async fn chat_ollama(&self, messages: &[ChatMessage], options: &ChatOptions) -> AiResult<String> {
        let body = OllamaChatRequestBody::new(&self.config, messages, options, false);
        let response = self
            .request()
            .json(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;

        let status = response.status();
        let text = response.text().await.map_err(|error| self.unreachable(error))?;
        if !status.is_success() {
            return Err(map_http_error(status.as_u16(), &text, &self.provider_name()));
        }
        let parsed: OllamaChatResponse = serde_json::from_str(&text).map_err(|_| AiError::InvalidResponse(self.provider_name()))?;
        if !parsed.error.trim().is_empty() {
            return Err(AiError::Server(self.provider_name(), parsed.error));
        }
        if parsed.message.content.trim().is_empty() {
            return Err(AiError::InvalidResponse(self.provider_name()));
        }
        Ok(parsed.message.content)
    }

    /// Streaming completion: `on_delta` receives incremental answer text and
    /// `on_reasoning` the thinking trace (when the model is in thinking mode),
    /// so the UI can render both while they are produced.
    pub async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        options: ChatOptions,
        cancel: &CancelToken,
        on_delta: &mut (dyn FnMut(&str) + Send),
        on_reasoning: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<String> {
        if self.provider() == ProviderKind::Ollama {
            let _ = on_reasoning;
            self.chat_stream_ollama(messages, options, cancel, on_delta).await
        } else {
            self.chat_stream_openai_compatible(messages, options, cancel, on_delta, on_reasoning)
                .await
        }
    }

    async fn chat_stream_openai_compatible(
        &self,
        messages: &[ChatMessage],
        options: ChatOptions,
        cancel: &CancelToken,
        on_delta: &mut (dyn FnMut(&str) + Send),
        on_reasoning: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<String> {
        let body = ChatRequestBody::new(&self.config, messages, &options, true);
        let response = self
            .request()
            .json(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &text, &self.provider_name()));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full = String::new();
        let mut reasoning_text = String::new();
        while let Some(chunk) = stream.next().await {
            cancel.check()?;
            let bytes = chunk.map_err(|error| self.unreachable(error))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));
            // Server-sent events: lines like `data: {...}` separated by blank lines.
            while let Some(position) = buffer.find('\n') {
                let line = buffer[..position].trim().to_string();
                buffer.drain(..=position);
                let Some(payload) = line.strip_prefix("data:") else {
                    continue;
                };
                let payload = payload.trim();
                if payload == "[DONE]" {
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<StreamChunk>(payload) {
                    if let Some(choice) = parsed.choices.into_iter().next() {
                        if let Some(reasoning) = choice.delta.reasoning_content {
                            if !reasoning.is_empty() {
                                reasoning_text.push_str(&reasoning);
                                on_reasoning(&reasoning);
                            }
                        }
                        if let Some(content) = choice.delta.content {
                            if !content.is_empty() {
                                full.push_str(&content);
                                on_delta(&content);
                            }
                        }
                    }
                }
            }
        }
        if full.trim().is_empty() {
            // Thinking mode can consume the whole budget before any answer
            // text is produced; returning the reasoning trace is more useful
            // than failing with "unexpected response".
            if !reasoning_text.trim().is_empty() {
                return Ok(reasoning_text);
            }
            return Err(AiError::InvalidResponse(self.provider_name()));
        }
        Ok(full)
    }

    async fn chat_stream_ollama(
        &self,
        messages: &[ChatMessage],
        options: ChatOptions,
        cancel: &CancelToken,
        on_delta: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<String> {
        let body = OllamaChatRequestBody::new(&self.config, messages, &options, true);
        let response = self
            .request()
            .json(&body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &text, &self.provider_name()));
        }

        // Ollama streams newline-delimited JSON objects, one per line:
        // {"message":{"content":"..."},"done":false} ... {"done":true}
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full = String::new();
        let mut finished = false;
        while let Some(chunk) = stream.next().await {
            cancel.check()?;
            let bytes = chunk.map_err(|error| self.unreachable(error))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));
            while let Some(position) = buffer.find('\n') {
                let line = buffer[..position].trim().to_string();
                buffer.drain(..=position);
                if line.is_empty() {
                    continue;
                }
                let parsed: OllamaChatResponse =
                    serde_json::from_str(&line).map_err(|_| AiError::InvalidResponse(self.provider_name()))?;
                if !parsed.error.trim().is_empty() {
                    return Err(AiError::Server(self.provider_name(), parsed.error));
                }
                if !parsed.message.content.is_empty() {
                    full.push_str(&parsed.message.content);
                    on_delta(&parsed.message.content);
                }
                if parsed.done {
                    finished = true;
                    break;
                }
            }
            if finished {
                break;
            }
        }
        if !finished {
            let line = buffer.trim();
            if !line.is_empty() {
                let parsed: OllamaChatResponse =
                    serde_json::from_str(line).map_err(|_| AiError::InvalidResponse(self.provider_name()))?;
                if !parsed.error.trim().is_empty() {
                    return Err(AiError::Server(self.provider_name(), parsed.error));
                }
                if !parsed.message.content.is_empty() {
                    full.push_str(&parsed.message.content);
                    on_delta(&parsed.message.content);
                }
            }
        }
        if full.trim().is_empty() {
            return Err(AiError::InvalidResponse(self.provider_name()));
        }
        Ok(full)
    }

    /// Cheap connectivity/credentials check for the settings screen.
    pub async fn test_connection(&self) -> AiResult<String> {
        let messages = vec![ChatMessage::user("Reply with the single word: ready")];
        let reply = self
            .chat(
                &messages,
                ChatOptions {
                    temperature: Some(0.0),
                    max_tokens: Some(16),
                },
            )
            .await?;
        Ok(reply.trim().to_string())
    }
}

fn map_http_error(status: u16, body: &str, provider: &str) -> AiError {
    let detail = serde_json::from_str::<ApiErrorBody>(body)
        .ok()
        .map(|parsed| parsed.error)
        .or_else(|| {
            // Ollama reports errors as {"error": "text"}.
            let value: serde_json::Value = serde_json::from_str(body).ok()?;
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(|message| ApiErrorDetail {
                    message: message.to_string(),
                    code: String::new(),
                    kind: String::new(),
                })
        });
    let code = detail
        .as_ref()
        .map(|error| format!("{} {}", error.code, error.kind).to_lowercase())
        .unwrap_or_default();
    let message = detail.map(|error| error.message).unwrap_or_default();
    match status {
        401 | 403 => AiError::InvalidApiKey(provider.to_string()),
        402 => AiError::InsufficientBalance(provider.to_string()),
        429 => AiError::RateLimited(provider.to_string()),
        400 if code.contains("context") || message.to_lowercase().contains("context") => AiError::TooLarge,
        _ => {
            let summary = if message.is_empty() {
                format!("HTTP {status}")
            } else {
                format!("HTTP {status}: {message}")
            };
            AiError::Server(provider.to_string(), summary)
        }
    }
}

/// Formats a server-sent stream of text deltas into the final answer while
/// keeping the caller informed. Kept as a small helper so the command layer
/// stays thin.
pub async fn run_plan(
    client: &DeepSeekClient,
    plan: &Plan,
    cancel: &CancelToken,
    on_progress: &mut (dyn FnMut(&str, usize, usize) + Send),
    on_delta: &mut (dyn FnMut(&str) + Send),
    on_reasoning: &mut (dyn FnMut(&str) + Send),
) -> AiResult<String> {
    match plan {
        Plan::Single { messages } => {
            on_progress("request", 0, 1);
            let text = client
                .chat_stream(messages, ChatOptions::default(), cancel, on_delta, on_reasoning)
                .await?;
            on_progress("request", 1, 1);
            Ok(text)
        }
        Plan::MapReduce { chunks, reduce } => {
            let total = chunks.len() + 1;
            let mut partials: Vec<String> = Vec::with_capacity(chunks.len());
            for (index, chunk) in chunks.iter().enumerate() {
                cancel.check()?;
                on_progress("chunk", index, total);
                let messages = vec![
                    ChatMessage::system(prompts::MAP_SYSTEM),
                    ChatMessage::user(chunk.clone()),
                ];
                let partial = client
                    .chat(
                        &messages,
                        ChatOptions {
                            temperature: Some(0.1),
                            max_tokens: Some(1024),
                        },
                    )
                    .await?;
                partials.push(partial);
            }
            on_progress("reduce", chunks.len(), total);
            let combined = partials.join("\n\n---\n\n");
            let messages = vec![
                ChatMessage::system(prompts::REDUCE_SYSTEM),
                ChatMessage::user(format!("{}\n\n{}", reduce, combined)),
            ];
            let final_text = client
                .chat_stream(&messages, ChatOptions::default(), cancel, on_delta, on_reasoning)
                .await?;
            on_progress("reduce", total, total);
            Ok(final_text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_url_requires_https_for_remote_hosts() {
        assert_eq!(normalize_base_url(" https://api.deepseek.com/ ").unwrap(), "https://api.deepseek.com");
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/").unwrap(),
            "https://api.openai.com/v1"
        );
        // A remote plain-HTTP endpoint would leak the API key: refused.
        assert!(matches!(
            normalize_base_url("http://api.example.com/v1"),
            Err(AiError::InvalidBaseUrl(_))
        ));
        assert!(matches!(
            normalize_base_url("http://192.168.1.10:8000/v1"),
            Err(AiError::InvalidBaseUrl(_))
        ));
        // Loopback HTTP stays available for local servers (Ollama).
        assert_eq!(
            normalize_base_url("http://localhost:11434").unwrap(),
            "http://localhost:11434"
        );
        assert_eq!(normalize_base_url("http://127.0.0.1:11434").unwrap(), "http://127.0.0.1:11434");
        assert_eq!(normalize_base_url("http://[::1]:11434").unwrap(), "http://[::1]:11434");
        // Other schemes, credentials, queries and fragments are rejected.
        assert!(normalize_base_url("ftp://host/v1").is_err());
        assert!(normalize_base_url("").is_err());
        assert!(normalize_base_url("https://user:key@host/v1").is_err());
        assert!(normalize_base_url("https://host/v1?x=1").is_err());
        assert!(normalize_base_url("https://host/v1#frag").is_err());
    }

    #[test]
    fn redirect_policy_rejects_public_http_targets() {
        let https = reqwest::Url::parse("https://api.deepseek.com/x").unwrap();
        let public_http = reqwest::Url::parse("http://api.example.com/x").unwrap();
        let loopback_http = reqwest::Url::parse("http://127.0.0.1:11434/x").unwrap();
        assert!(is_redirect_target_allowed(&https, false));
        assert!(!is_redirect_target_allowed(&public_http, false));
        assert!(!is_redirect_target_allowed(&public_http, true));
        assert!(!is_redirect_target_allowed(&loopback_http, false));
        assert!(is_redirect_target_allowed(&loopback_http, true));
    }

    #[test]
    fn client_refuses_a_remote_http_base_url() {
        let error = DeepSeekClient::new(AiConfig {
            api_key: "test-key".into(),
            base_url: "http://api.example.com/v1".into(),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(error, AiError::InvalidBaseUrl(_)));
    }

    #[test]
    fn client_normalizes_the_stored_base_url() {
        let client = DeepSeekClient::new(AiConfig {
            api_key: "test-key".into(),
            base_url: " https://api.deepseek.com/ ".into(),
            ..Default::default()
        })
        .expect("client");
        assert_eq!(client.config().base_url, "https://api.deepseek.com");
    }
}

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
    #[error("the API key was rejected by DeepSeek")]
    InvalidApiKey,
    #[error("DeepSeek rate limit reached, try again in a moment")]
    RateLimited,
    #[error("the DeepSeek account has insufficient balance")]
    InsufficientBalance,
    #[error("could not reach api.deepseek.com")]
    Network,
    #[error("DeepSeek returned an error: {0}")]
    Server(String),
    #[error("unexpected response from DeepSeek")]
    InvalidResponse,
    #[error("operation cancelled")]
    Cancelled,
    #[error("the document has no extractable text (run OCR first)")]
    NoText,
    #[error("request was too large for the model context")]
    TooLarge,
    #[error("{0}")]
    ProviderUnreachable(String),
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
pub struct DeepSeekClient {
    config: AiConfig,
    http: reqwest::Client,
}

impl DeepSeekClient {
    pub fn new(config: AiConfig) -> AiResult<Self> {
        if !config.is_configured() {
            return Err(AiError::MissingApiKey);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .connect_timeout(Duration::from_secs(20))
            .user_agent(concat!("PDFSwissArmyKnife/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| AiError::Network)?;
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
            return Err(map_http_error(status.as_u16(), &text));
        }
        let parsed: ChatResponseBody = serde_json::from_str(&text).map_err(|_| AiError::InvalidResponse)?;
        parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.text())
            .ok_or(AiError::InvalidResponse)
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
            return Err(map_http_error(status.as_u16(), &text));
        }
        let parsed: OllamaChatResponse = serde_json::from_str(&text).map_err(|_| AiError::InvalidResponse)?;
        if !parsed.error.trim().is_empty() {
            return Err(AiError::Server(parsed.error));
        }
        if parsed.message.content.trim().is_empty() {
            return Err(AiError::InvalidResponse);
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
            return Err(map_http_error(status.as_u16(), &text));
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
            return Err(AiError::InvalidResponse);
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
            return Err(map_http_error(status.as_u16(), &text));
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
                    serde_json::from_str(&line).map_err(|_| AiError::InvalidResponse)?;
                if !parsed.error.trim().is_empty() {
                    return Err(AiError::Server(parsed.error));
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
                    serde_json::from_str(line).map_err(|_| AiError::InvalidResponse)?;
                if !parsed.error.trim().is_empty() {
                    return Err(AiError::Server(parsed.error));
                }
                if !parsed.message.content.is_empty() {
                    full.push_str(&parsed.message.content);
                    on_delta(&parsed.message.content);
                }
            }
        }
        if full.trim().is_empty() {
            return Err(AiError::InvalidResponse);
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

fn map_http_error(status: u16, body: &str) -> AiError {
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
        401 | 403 => AiError::InvalidApiKey,
        402 => AiError::InsufficientBalance,
        429 => AiError::RateLimited,
        400 if code.contains("context") || message.to_lowercase().contains("context") => AiError::TooLarge,
        _ => {
            let summary = if message.is_empty() {
                format!("HTTP {status}")
            } else {
                format!("HTTP {status}: {message}")
            };
            AiError::Server(summary)
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

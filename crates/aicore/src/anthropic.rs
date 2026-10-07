//! Anthropic (Claude) Messages API support.
//!
//! Everything here is pure (no I/O) so it can be unit tested without a
//! network: request-body building, response/stream parsing, error mapping and
//! model-listing parsing. The HTTP plumbing lives in `DeepSeekClient`.
//!
//! API shape (raw HTTP, there is no official Rust SDK):
//! - `POST {base}/v1/messages` with `x-api-key`, `anthropic-version` and a JSON
//!   body (`model`, `max_tokens`, top-level `system`, `messages`, `stream`).
//! - `GET {base}/v1/models?limit=100[&after_id=..]` for model discovery.
//!
//! Sampling parameters (`temperature`, `top_p`, `top_k`) and the `thinking`
//! field are deliberately never sent: current Claude models reject sampling
//! parameters and think adaptively on their own.

use crate::{AiConfig, AiError, AiResult, ChatMessage, ChatOptions, ModelInfo, MAX_ERROR_MESSAGE_CHARS};
use serde::{Deserialize, Serialize};

pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";
/// Output cap used for Claude requests. Lower than the DeepSeek cap because a
/// larger `max_tokens` is rejected by the smaller Claude models.
pub const ANTHROPIC_MAX_OUTPUT_TOKENS: u32 = 64_000;
/// Smallest `max_tokens` sent to Claude. Thinking is always on for the newer
/// models and counts against the budget, so a few hundred tokens (connection
/// test, short metadata call) can be consumed before any answer text appears.
pub const ANTHROPIC_MIN_OUTPUT_TOKENS: u32 = 2_048;
/// Model ids offered in the settings (alias ids, never date-suffixed ones).
pub const ANTHROPIC_MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5", "Claude Opus 5.5 (highest quality, recommended)"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5 (balanced speed and quality)"),
    ("claude-haiku-4-5", "Claude Haiku 4.5 (fastest, lowest cost)"),
];
/// Page size and page cap for `GET /v1/models` (100 per page, 10 pages).
pub const MODELS_PAGE_LIMIT: u32 = 100;
pub const MODELS_MAX_PAGES: usize = 10;

/// `output_config.effort` values the Messages API accepts.
const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct AnthropicMessage {
    pub role: &'static str,
    pub content: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct OutputConfig {
    pub effort: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct MessagesRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub messages: Vec<AnthropicMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
}

/// Maps the stored `reasoning_effort` onto an `output_config.effort` value.
///
/// Always sent when the value is valid and the model supports it (Haiku does
/// not): the server default differs per model (`medium` on Opus 5.5), so
/// "high" must be sent explicitly to mean high. Returning `None` omits the
/// field.
pub(crate) fn effort_for(config: &AiConfig) -> Option<&'static str> {
    if config.model.to_lowercase().contains("haiku") {
        return None;
    }
    let wanted = config.reasoning_effort.trim().to_lowercase();
    let level = EFFORT_LEVELS.iter().copied().find(|level| *level == wanted)?;
    Some(level)
}

/// Builds the request. `system`-role messages move into the top-level
/// `system` string; consecutive same-role messages are merged because the API
/// requires alternating roles.
pub(crate) fn build_request<'a>(
    config: &'a AiConfig,
    messages: &[ChatMessage],
    options: &ChatOptions,
    stream: bool,
) -> MessagesRequest<'a> {
    let mut system_parts: Vec<&str> = Vec::new();
    let mut merged: Vec<AnthropicMessage> = Vec::new();
    for message in messages {
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        let role = match message.role.trim().to_lowercase().as_str() {
            "system" | "developer" => {
                system_parts.push(content);
                continue;
            }
            "assistant" => "assistant",
            _ => "user",
        };
        match merged.last_mut() {
            Some(last) if last.role == role => {
                last.content.push_str("\n\n");
                last.content.push_str(content);
            }
            _ => merged.push(AnthropicMessage { role, content: content.to_string() }),
        }
    }
    let mut system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    if merged.is_empty() {
        // A request needs at least one user turn; a lone system prompt becomes it.
        if let Some(text) = system.take() {
            merged.push(AnthropicMessage { role: "user", content: text });
        }
    }
    let max_tokens =
        options.max_tokens.unwrap_or(config.max_tokens).clamp(ANTHROPIC_MIN_OUTPUT_TOKENS, ANTHROPIC_MAX_OUTPUT_TOKENS);
    MessagesRequest {
        model: &config.model,
        max_tokens,
        system,
        messages: merged,
        stream,
        output_config: effort_for(config).map(|effort| OutputConfig { effort }),
    }
}

/// `{base}/v1/messages` (a base that already ends in `/v1` is not doubled).
pub(crate) fn messages_endpoint(base: &str) -> String {
    format!("{}/messages", versioned_base(base))
}

/// `{base}/v1/models`.
pub(crate) fn models_endpoint(base: &str) -> String {
    format!("{}/models", versioned_base(base))
}

fn versioned_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/v1") {
        base.to_string()
    } else {
        format!("{base}/v1")
    }
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

/// Parses a non-streaming response: concatenates the `text` blocks (thinking
/// blocks are ignored). A `refusal` stop reason is an error, and so is
/// `max_tokens`: an answer that was cut off must never pass as complete.
pub(crate) fn parse_message_response(body: &str, provider: &str) -> AiResult<String> {
    let parsed: MessagesResponse =
        serde_json::from_str(body).map_err(|_| AiError::InvalidResponse(provider.to_string()))?;
    if parsed.stop_reason.as_deref() == Some("refusal") {
        return Err(refusal_error(provider));
    }
    if parsed.stop_reason.as_deref() == Some("max_tokens") {
        return Err(AiError::Truncated(provider.to_string()));
    }
    let text: String =
        parsed.content.iter().filter(|block| block.kind == "text").map(|block| block.text.as_str()).collect();
    if text.trim().is_empty() {
        return Err(AiError::InvalidResponse(provider.to_string()));
    }
    Ok(text)
}

fn refusal_error(provider: &str) -> AiError {
    AiError::Server(provider.to_string(), "The model declined this request".to_string())
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    error: ErrorDetail,
}

#[derive(Debug, Deserialize, Default)]
struct ErrorDetail {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    message: String,
}

fn capped(message: &str) -> String {
    message.chars().take(MAX_ERROR_MESSAGE_CHARS).collect()
}

/// True only when an `invalid_request_error` message clearly says the prompt
/// or context is too long. A bare mention of "context" (an unknown
/// `context_management` field, for example) is an ordinary request error.
fn says_context_too_long(lowered: &str) -> bool {
    if lowered.contains("prompt is too long") || lowered.contains("input is too long") {
        return true;
    }
    lowered.contains("context")
        && ["too long", "too large", "exceed", "window", "length"].iter().any(|word| lowered.contains(word))
}

/// Maps an Anthropic error type (`{"type":"error","error":{"type":..}}`) to the
/// shared error enum. Used for both HTTP error bodies and in-stream `error`
/// events.
pub(crate) fn map_error_kind(kind: &str, message: &str, provider: &str) -> AiError {
    let lowered = message.to_lowercase();
    match kind {
        "authentication_error" | "permission_error" => AiError::InvalidApiKey(provider.to_string()),
        "rate_limit_error" => AiError::RateLimited(provider.to_string()),
        "billing_error" => AiError::InsufficientBalance(provider.to_string()),
        "request_too_large" => AiError::TooLarge,
        "overloaded_error" => AiError::Server(
            provider.to_string(),
            "the service is temporarily overloaded, try again in a moment".to_string(),
        ),
        "invalid_request_error" if lowered.contains("credit balance") => {
            AiError::InsufficientBalance(provider.to_string())
        }
        "invalid_request_error" if says_context_too_long(&lowered) => AiError::TooLarge,
        _ => {
            let message = capped(message);
            let summary = match (kind.is_empty(), message.is_empty()) {
                (_, false) => message,
                (false, true) => kind.to_string(),
                (true, true) => "unknown error".to_string(),
            };
            AiError::Server(provider.to_string(), summary)
        }
    }
}

/// Maps a non-2xx response. A recognised Anthropic error body wins; otherwise
/// the status code decides.
pub(crate) fn map_http_error(status: u16, body: &str, provider: &str) -> AiError {
    if let Ok(envelope) = serde_json::from_str::<ErrorEnvelope>(body) {
        if !envelope.error.kind.is_empty() {
            return map_error_kind(&envelope.error.kind, &envelope.error.message, provider);
        }
    }
    match status {
        401 | 403 => AiError::InvalidApiKey(provider.to_string()),
        402 => AiError::InsufficientBalance(provider.to_string()),
        413 => AiError::TooLarge,
        429 => AiError::RateLimited(provider.to_string()),
        _ => crate::map_http_error(status, body, provider),
    }
}

/// Incremental parser for the Messages server-sent-event stream.
///
/// Bytes are buffered (so a multi-byte character or an event split across
/// network reads is reassembled) and decoded line by line. Text and thinking
/// deltas are reported through the callbacks as they arrive.
#[derive(Debug, Default)]
pub(crate) struct StreamParser {
    buffer: Vec<u8>,
    pub text: String,
    pub thinking: String,
    stop_reason: Option<String>,
    /// True once the `message_stop` event arrived (the stream ended cleanly).
    saw_stop: bool,
    /// Bytes at the start of `buffer` already known to hold no newline, so a
    /// long line is not rescanned from byte 0 on every network read.
    scanned: usize,
}

impl StreamParser {
    /// Bytes currently held (used for the response size cap).
    pub fn held_bytes(&self) -> usize {
        self.buffer.len() + self.text.len() + self.thinking.len()
    }

    pub fn feed(
        &mut self,
        bytes: &[u8],
        provider: &str,
        on_text: &mut (dyn FnMut(&str) + Send),
        on_thinking: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<()> {
        self.buffer.extend_from_slice(bytes);
        loop {
            let Some(offset) = self.buffer[self.scanned..].iter().position(|byte| *byte == b'\n') else {
                self.scanned = self.buffer.len();
                return Ok(());
            };
            let position = self.scanned + offset;
            let line: Vec<u8> = self.buffer.drain(..=position).collect();
            self.scanned = 0;
            self.handle_line(&line, provider, on_text, on_thinking)?;
        }
    }

    /// Processes a final line that was not newline-terminated and returns the
    /// accumulated answer. Fails on a refusal, when the answer was cut off
    /// (`max_tokens`, or the stream ended without `message_stop`) or when no
    /// text was produced.
    pub fn finish(
        mut self,
        provider: &str,
        on_text: &mut (dyn FnMut(&str) + Send),
        on_thinking: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<String> {
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.handle_line(&line, provider, on_text, on_thinking)?;
        }
        if self.stop_reason.as_deref() == Some("refusal") {
            return Err(refusal_error(provider));
        }
        if self.stop_reason.as_deref() == Some("max_tokens") {
            return Err(AiError::Truncated(provider.to_string()));
        }
        if self.text.trim().is_empty() {
            return Err(AiError::InvalidResponse(provider.to_string()));
        }
        if !self.saw_stop {
            return Err(AiError::Truncated(provider.to_string()));
        }
        Ok(self.text)
    }

    fn handle_line(
        &mut self,
        line: &[u8],
        provider: &str,
        on_text: &mut (dyn FnMut(&str) + Send),
        on_thinking: &mut (dyn FnMut(&str) + Send),
    ) -> AiResult<()> {
        let line = String::from_utf8_lossy(line);
        let Some(payload) = line.trim().strip_prefix("data:") else {
            // `event:` names, comments and blank separators carry nothing the
            // JSON payload does not repeat in its own `type`.
            return Ok(());
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(payload.trim()) else {
            return Ok(());
        };
        match value.get("type").and_then(|kind| kind.as_str()).unwrap_or_default() {
            "content_block_delta" => {
                let delta = value.get("delta");
                match delta.and_then(|delta| delta.get("type")).and_then(|kind| kind.as_str()) {
                    Some("text_delta") => {
                        if let Some(text) = delta.and_then(|delta| delta.get("text")).and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                self.text.push_str(text);
                                on_text(text);
                            }
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(text) = delta.and_then(|delta| delta.get("thinking")).and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                self.thinking.push_str(text);
                                on_thinking(text);
                            }
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(reason) =
                    value.get("delta").and_then(|delta| delta.get("stop_reason")).and_then(|reason| reason.as_str())
                {
                    self.stop_reason = Some(reason.to_string());
                }
            }
            "message_stop" => self.saw_stop = true,
            "error" => {
                let error = value.get("error");
                let kind = error.and_then(|e| e.get("type")).and_then(|k| k.as_str()).unwrap_or_default();
                let message = error.and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or_default();
                return Err(map_error_kind(kind, message, provider));
            }
            // message_start, content_block_start/stop, ping and
            // any event type added later are not needed to assemble the text.
            _ => {}
        }
        Ok(())
    }
}

/// One page of `GET /v1/models`: the models and, when more pages exist, the
/// `after_id` cursor for the next request.
pub(crate) fn parse_models_page(body: &str) -> (Vec<ModelInfo>, Option<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return (Vec::new(), None);
    };
    let models = value
        .get("data")
        .and_then(|data| data.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("id").and_then(|id| id.as_str())?.trim();
                    if id.is_empty() {
                        return None;
                    }
                    let label = entry
                        .get("display_name")
                        .and_then(|name| name.as_str())
                        .filter(|name| !name.trim().is_empty())
                        .map(|name| name.to_string());
                    let number = |key: &str| {
                        entry.get(key).and_then(|value| value.as_u64()).and_then(|value| u32::try_from(value).ok())
                    };
                    Some(ModelInfo {
                        id: id.to_string(),
                        label,
                        context_tokens: number("max_input_tokens"),
                        max_output_tokens: number("max_tokens"),
                        local: false,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let has_more = value.get("has_more").and_then(|more| more.as_bool()).unwrap_or(false);
    let next = if has_more {
        value.get("last_id").and_then(|id| id.as_str()).filter(|id| !id.is_empty()).map(|id| id.to_string())
    } else {
        None
    };
    (models, next)
}

/// Sorts and de-duplicates a merged model list.
pub(crate) fn finish_models(mut models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> AiConfig {
        AiConfig {
            api_key: "sk-ant-test".into(),
            base_url: ANTHROPIC_BASE_URL.into(),
            model: "claude-opus-5-5".into(),
            provider: crate::ProviderKind::Anthropic,
            ..Default::default()
        }
    }

    fn body_json(
        config: &AiConfig,
        messages: &[ChatMessage],
        options: &ChatOptions,
        stream: bool,
    ) -> serde_json::Value {
        serde_json::to_value(build_request(config, messages, options, stream)).unwrap()
    }

    #[test]
    fn request_has_top_level_system_and_no_sampling_or_thinking_fields() {
        let config = AiConfig { thinking: true, ..config() };
        let messages = [ChatMessage::system("Be brief."), ChatMessage::user("Hi")];
        let body =
            body_json(&config, &messages, &ChatOptions { temperature: Some(0.3), max_tokens: Some(3000) }, false);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["max_tokens"], 3000);
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"], json!([{"role": "user", "content": "Hi"}]));
        for forbidden in ["temperature", "top_p", "top_k", "thinking"] {
            assert!(body.get(forbidden).is_none(), "{forbidden} must not be sent: {body}");
        }
        // "high" is sent explicitly: the server default is model dependent.
        assert_eq!(body["output_config"], json!({"effort": "high"}));
    }

    #[test]
    fn same_role_messages_are_merged_and_system_messages_collected() {
        let messages = [
            ChatMessage::system("First rule."),
            ChatMessage::user("a"),
            ChatMessage::user("b"),
            ChatMessage { role: "assistant".into(), content: "c".into() },
            ChatMessage::system("Second rule."),
            ChatMessage { role: "assistant".into(), content: "d".into() },
            ChatMessage::user("   "),
            ChatMessage::user("e"),
        ];
        let body = body_json(&config(), &messages, &ChatOptions::default(), true);
        assert_eq!(body["system"], "First rule.\n\nSecond rule.");
        assert_eq!(body["stream"], true);
        assert_eq!(
            body["messages"],
            json!([
                {"role": "user", "content": "a\n\nb"},
                {"role": "assistant", "content": "c\n\nd"},
                {"role": "user", "content": "e"},
            ])
        );
    }

    #[test]
    fn a_lone_system_prompt_becomes_the_user_turn() {
        let body = body_json(&config(), &[ChatMessage::system("Only this.")], &ChatOptions::default(), false);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"], json!([{"role": "user", "content": "Only this."}]));
    }

    #[test]
    fn max_tokens_is_clamped_to_the_claude_range() {
        let low = body_json(
            &config(),
            &[ChatMessage::user("x")],
            &ChatOptions { max_tokens: Some(1), ..Default::default() },
            false,
        );
        // Thinking can eat a tiny budget, so Claude calls never go below 2048.
        assert_eq!(low["max_tokens"], ANTHROPIC_MIN_OUTPUT_TOKENS);
        assert_eq!(low["max_tokens"], 2048);
        let high = body_json(
            &config(),
            &[ChatMessage::user("x")],
            &ChatOptions { max_tokens: Some(384_000), ..Default::default() },
            false,
        );
        assert_eq!(high["max_tokens"], ANTHROPIC_MAX_OUTPUT_TOKENS);
        let default = body_json(&config(), &[ChatMessage::user("x")], &ChatOptions::default(), false);
        assert_eq!(default["max_tokens"], 4096);
    }

    #[test]
    fn effort_is_mapped_validated_and_omitted_when_unsupported() {
        let with = |effort: &str, model: &str, thinking: bool| AiConfig {
            reasoning_effort: effort.into(),
            model: model.into(),
            thinking,
            ..config()
        };
        assert_eq!(effort_for(&with("low", "claude-opus-5-5", true)), Some("low"));
        assert_eq!(effort_for(&with(" XHigh ", "claude-opus-5-5", true)), Some("xhigh"));
        assert_eq!(effort_for(&with("max", "claude-sonnet-5-5", true)), Some("max"));
        assert_eq!(effort_for(&with("medium", "claude-sonnet-5-5", true)), Some("medium"));
        assert_eq!(effort_for(&with("high", "claude-opus-5-5", true)), Some("high"));
        assert_eq!(effort_for(&with("high", "claude-haiku-4-5", true)), None);
        assert_eq!(effort_for(&with("turbo", "claude-opus-5-5", true)), None);
        assert_eq!(effort_for(&with("low", "claude-haiku-4-5", true)), None);
        // The DeepSeek-only thinking toggle does not gate the effort.
        assert_eq!(effort_for(&with("low", "claude-opus-5-5", false)), Some("low"));

        let body =
            body_json(&with("max", "claude-opus-5-5", true), &[ChatMessage::user("x")], &ChatOptions::default(), false);
        assert_eq!(body["output_config"], json!({"effort": "max"}));
    }

    #[test]
    fn endpoints_do_not_double_the_version_segment() {
        assert_eq!(messages_endpoint("https://api.anthropic.com"), "https://api.anthropic.com/v1/messages");
        assert_eq!(messages_endpoint("https://api.anthropic.com/"), "https://api.anthropic.com/v1/messages");
        assert_eq!(messages_endpoint("https://proxy.example/v1"), "https://proxy.example/v1/messages");
        assert_eq!(models_endpoint("https://api.anthropic.com"), "https://api.anthropic.com/v1/models");
    }

    #[test]
    fn response_text_blocks_are_joined_and_thinking_is_ignored() {
        let body = r#"{"id":"msg_1","type":"message","role":"assistant","stop_reason":"end_turn","content":[
            {"type":"thinking","thinking":"hidden reasoning","signature":"abc"},
            {"type":"text","text":"Hello"},
            {"type":"text","text":" world"}]}"#;
        assert_eq!(parse_message_response(body, "Anthropic (Claude)").unwrap(), "Hello world");
    }

    #[test]
    fn max_tokens_stop_is_a_truncation_error() {
        let body = r#"{"stop_reason":"max_tokens","content":[{"type":"text","text":"partial answ"}]}"#;
        assert!(matches!(parse_message_response(body, "Anthropic (Claude)"), Err(AiError::Truncated(_))));
    }

    #[test]
    fn refusals_and_empty_or_invalid_bodies_are_errors() {
        let refusal = r#"{"stop_reason":"refusal","content":[{"type":"text","text":"I can't"}]}"#;
        match parse_message_response(refusal, "Anthropic (Claude)") {
            Err(AiError::Server(provider, message)) => {
                assert_eq!(provider, "Anthropic (Claude)");
                assert!(message.contains("declined"), "{message}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        let thinking_only = r#"{"stop_reason":"end_turn","content":[{"type":"thinking","thinking":"hm"}]}"#;
        assert!(matches!(parse_message_response(thinking_only, "A"), Err(AiError::InvalidResponse(_))));
        assert!(matches!(parse_message_response("not json", "A"), Err(AiError::InvalidResponse(_))));
        assert!(matches!(parse_message_response("{}", "A"), Err(AiError::InvalidResponse(_))));
    }

    fn run_stream(chunks: &[&[u8]]) -> (AiResult<String>, String, String) {
        let mut parser = StreamParser::default();
        let mut text = String::new();
        let mut thinking = String::new();
        let mut result = Ok(());
        for chunk in chunks {
            result = parser.feed(chunk, "Anthropic (Claude)", &mut |t| text.push_str(t), &mut |t| thinking.push_str(t));
            if result.is_err() {
                break;
            }
        }
        let outcome = match result {
            Ok(()) => parser.finish("Anthropic (Claude)", &mut |t| text.push_str(t), &mut |t| thinking.push_str(t)),
            Err(error) => Err(error),
        };
        (outcome, text, thinking)
    }

    const SAMPLE_STREAM: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"content\":[]}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"Let me check.\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"wörld\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );

    #[test]
    fn stream_text_and_thinking_deltas_are_reported_separately() {
        let (outcome, text, thinking) = run_stream(&[SAMPLE_STREAM.as_bytes()]);
        assert_eq!(outcome.unwrap(), "Hello wörld");
        assert_eq!(text, "Hello wörld");
        // The empty thinking_delta must not produce a reasoning chunk.
        assert_eq!(thinking, "Let me check.");
    }

    #[test]
    fn stream_survives_events_and_characters_split_across_reads() {
        let bytes = SAMPLE_STREAM.as_bytes();
        // One byte per read also splits the two-byte "ö" in the middle.
        let chunks: Vec<&[u8]> = bytes.chunks(1).collect();
        let (outcome, text, thinking) = run_stream(&chunks);
        assert_eq!(outcome.unwrap(), "Hello wörld");
        assert_eq!(text, "Hello wörld");
        assert_eq!(thinking, "Let me check.");

        let odd: Vec<&[u8]> = bytes.chunks(37).collect();
        assert_eq!(run_stream(&odd).0.unwrap(), "Hello wörld");
    }

    #[test]
    fn stream_without_a_trailing_newline_still_delivers_the_last_event() {
        let stream = concat!(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"tail\"}}\n\n",
            "data: {\"type\":\"message_stop\"}"
        );
        assert_eq!(run_stream(&[stream.as_bytes()]).0.unwrap(), "tail");
    }

    #[test]
    fn stream_cut_off_by_max_tokens_or_a_dropped_connection_is_truncated() {
        let delta =
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"part\"}}\n\n";
        let max_tokens = format!(
            "{delta}data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"max_tokens\"}}}}\n\ndata: {{\"type\":\"message_stop\"}}\n\n"
        );
        let (outcome, text, _) = run_stream(&[max_tokens.as_bytes()]);
        assert_eq!(text, "part");
        assert!(matches!(outcome, Err(AiError::Truncated(_))), "{outcome:?}");

        // No message_stop: the connection ended mid-answer.
        let dropped =
            format!("{delta}data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"end_turn\"}}}}\n\n");
        assert!(matches!(run_stream(&[dropped.as_bytes()]).0, Err(AiError::Truncated(_))));
        assert!(matches!(run_stream(&[delta.as_bytes()]).0, Err(AiError::Truncated(_))));
    }

    #[test]
    fn long_lines_are_assembled_across_many_small_reads() {
        // One enormous text delta fed in tiny pieces: the newline scan resumes
        // where it stopped, and the result is still exact.
        let big = "x".repeat(50_000);
        let stream = format!(
            "data: {{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":\"{big}\"}}}}\n\ndata: {{\"type\":\"message_stop\"}}\n\n"
        );
        let chunks: Vec<&[u8]> = stream.as_bytes().chunks(7).collect();
        assert_eq!(run_stream(&chunks).0.unwrap(), big);
    }

    #[test]
    fn stream_error_event_maps_to_a_typed_error() {
        let stream = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"par\"}}\n\n",
            "event: error\n",
            "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n"
        );
        let (outcome, text, _) = run_stream(&[stream.as_bytes()]);
        assert_eq!(text, "par");
        assert!(matches!(outcome, Err(AiError::Server(_, _))), "{outcome:?}");

        let auth =
            "data: {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"bad key\"}}\n\n";
        assert!(matches!(run_stream(&[auth.as_bytes()]).0, Err(AiError::InvalidApiKey(_))));
        let rate = "data: {\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\",\"message\":\"slow down\"}}\n\n";
        assert!(matches!(run_stream(&[rate.as_bytes()]).0, Err(AiError::RateLimited(_))));
    }

    #[test]
    fn stream_refusal_and_empty_streams_fail() {
        let refusal = concat!(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"I\"}}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"}}\n\n"
        );
        match run_stream(&[refusal.as_bytes()]).0 {
            Err(AiError::Server(_, message)) => assert!(message.contains("declined"), "{message}"),
            other => panic!("unexpected: {other:?}"),
        }
        let empty = "data: {\"type\":\"message_stop\"}\n\n";
        assert!(matches!(run_stream(&[empty.as_bytes()]).0, Err(AiError::InvalidResponse(_))));
        // Garbage lines and unknown event types are ignored.
        let noisy = "data: not json\n: comment\nevent: future\ndata: {\"type\":\"future_event\"}\n\n";
        assert!(matches!(run_stream(&[noisy.as_bytes()]).0, Err(AiError::InvalidResponse(_))));
    }

    fn http_error(status: u16, kind: &str, message: &str) -> AiError {
        let body = json!({"type": "error", "error": {"type": kind, "message": message}}).to_string();
        map_http_error(status, &body, "Anthropic (Claude)")
    }

    #[test]
    fn error_bodies_map_to_the_shared_error_types() {
        assert!(matches!(http_error(401, "authentication_error", "invalid x-api-key"), AiError::InvalidApiKey(_)));
        assert!(matches!(http_error(403, "permission_error", "not allowed"), AiError::InvalidApiKey(_)));
        assert!(matches!(http_error(429, "rate_limit_error", "slow"), AiError::RateLimited(_)));
        assert!(matches!(http_error(402, "billing_error", "billing"), AiError::InsufficientBalance(_)));
        assert!(matches!(
            http_error(400, "invalid_request_error", "Your credit balance is too low to access the API"),
            AiError::InsufficientBalance(_)
        ));
        assert!(matches!(
            http_error(400, "invalid_request_error", "prompt is too long: 250000 tokens > 200000 maximum"),
            AiError::TooLarge
        ));
        assert!(matches!(http_error(413, "request_too_large", "too big"), AiError::TooLarge));
        match http_error(529, "overloaded_error", "Overloaded") {
            AiError::Server(provider, message) => {
                assert_eq!(provider, "Anthropic (Claude)");
                assert!(message.contains("overloaded"), "{message}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        match http_error(404, "not_found_error", "model: claude-nope") {
            AiError::Server(_, message) => assert_eq!(message, "model: claude-nope"),
            other => panic!("unexpected: {other:?}"),
        }
        // Only a message that clearly says the prompt/context is too long maps
        // to TooLarge; a stray mention of "context" does not.
        assert!(matches!(
            http_error(400, "invalid_request_error", "input length and max_tokens exceed context window size"),
            AiError::TooLarge
        ));
        match http_error(400, "invalid_request_error", "context_management: Extra inputs are not permitted") {
            AiError::Server(_, message) => assert!(message.contains("context_management"), "{message}"),
            other => panic!("unexpected: {other:?}"),
        }
        match http_error(400, "invalid_request_error", "messages: field required") {
            AiError::Server(_, message) => assert_eq!(message, "messages: field required"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn error_messages_are_length_capped_and_unknown_bodies_fall_back_to_the_status() {
        let long = "x".repeat(5000);
        match http_error(400, "invalid_request_error", &long) {
            AiError::Server(_, message) => assert_eq!(message.chars().count(), MAX_ERROR_MESSAGE_CHARS),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(map_http_error(401, "<html>nope</html>", "A"), AiError::InvalidApiKey(_)));
        assert!(matches!(map_http_error(429, "", "A"), AiError::RateLimited(_)));
        match map_http_error(502, "bad gateway", "A") {
            AiError::Server(_, message) => assert_eq!(message, "HTTP 502"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn models_page_parses_entries_and_pagination_cursor() {
        let first = r#"{"data":[
            {"type":"model","id":"claude-sonnet-5-5","display_name":"Claude Sonnet 5.5","created_at":"2026-01-01T00:00:00Z"},
            {"type":"model","id":"claude-opus-5-5","display_name":"Claude Opus 5.5","max_input_tokens":1000000,"max_tokens":128000},
            {"type":"model","id":"  "}],
            "has_more":true,"first_id":"claude-sonnet-5-5","last_id":"claude-opus-5-5"}"#;
        let (models, next) = parse_models_page(first);
        assert_eq!(next.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].label.as_deref(), Some("Claude Sonnet 5.5"));
        assert_eq!(models[1].context_tokens, Some(1_000_000));
        assert_eq!(models[1].max_output_tokens, Some(128_000));

        let last = r#"{"data":[{"id":"claude-haiku-4-5","display_name":"Claude Haiku 4.5"}],"has_more":false,"last_id":"claude-haiku-4-5"}"#;
        let (more, next) = parse_models_page(last);
        assert_eq!(next, None);
        let merged = finish_models(
            [
                models,
                more,
                vec![ModelInfo {
                    id: "claude-opus-5-5".into(),
                    label: None,
                    context_tokens: None,
                    max_output_tokens: None,
                    local: false,
                }],
            ]
            .concat(),
        );
        let ids: Vec<&str> = merged.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids, ["claude-haiku-4-5", "claude-opus-5-5", "claude-sonnet-5-5"]);

        assert_eq!(parse_models_page("garbage"), (Vec::new(), None));
        assert_eq!(parse_models_page(r#"{"error":{}}"#), (Vec::new(), None));
    }
}

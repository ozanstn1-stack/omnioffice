use super::*;

/// Fuzz entry point for all untrusted AI provider response parsers:
/// model discovery payloads (OpenAI/DeepSeek, Ollama, Gemini, Anthropic),
/// non-streaming and streaming chat chunks (OpenAI SSE, Ollama NDJSON,
/// Anthropic SSE), HTTP error bodies, and citation tags.
pub fn fuzz_ai_responses(data: &[u8]) {
    if data.len() > 64 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let _ = parse_openai_models(&text);
    let _ = parse_ollama_models(&text);
    let _ = parse_gemini_models(&text);
    let (models, _) = anthropic::parse_models_page(&text);
    let _ = anthropic::finish_models(models);
    let _ = anthropic::parse_message_response(&text, "Anthropic");
    let _ = serde_json::from_str::<ChatResponseBody>(&text);
    let _ = serde_json::from_str::<OllamaChatResponse>(&text);
    let _ = serde_json::from_str::<StreamChunk>(&text);
    let _ = map_http_error(400, &text, "Provider", Some(5));
    let _ = map_http_error(429, &text, "Provider", None);
    let _ = anthropic::map_http_error(400, &text, "Anthropic", Some(3));
    let _ = citations::parse_citations(&text);
    let _ = citations::parse_citations_expanded(&text);
    let _ = citations::quote_matches(&text, "ornek metin");

    let mut parser = anthropic::StreamParser::default();
    let mut on_delta = |_: &str| {};
    let mut on_reasoning = |_: &str| {};
    for chunk in data.chunks(97) {
        if parser.feed(chunk, "Anthropic", &mut on_delta, &mut on_reasoning).is_err() {
            return;
        }
    }
    let _ = parser.finish("Anthropic", &mut on_delta, &mut on_reasoning);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_model_listings_are_parsed_in_both_shapes() {
        let compat = r#"{"object":"list","data":[
            {"id":"models/gemini-2.5-flash","object":"model","owned_by":"google"},
            {"id":"models/gemini-2.0-flash","object":"model","owned_by":"google"},
            {"id":"models/gemini-2.0-flash","object":"model"}]}"#;
        let ids: Vec<String> = parse_gemini_models(compat).into_iter().map(|model| model.id).collect();
        assert_eq!(ids, ["gemini-2.0-flash", "gemini-2.5-flash"]);

        let native = r#"{"models":[
            {"name":"models/gemini-2.5-pro","displayName":"Gemini 2.5 Pro","inputTokenLimit":1048576,"outputTokenLimit":65536,
             "supportedGenerationMethods":["generateContent","countTokens"]},
            {"name":"models/text-embedding-004","supportedGenerationMethods":["embedContent"]},
            {"name":"models/gemini-2.0-flash"}]}"#;
        let models = parse_gemini_models(native);
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids, ["gemini-2.0-flash", "gemini-2.5-pro"]);
        assert_eq!(models[1].label.as_deref(), Some("Gemini 2.5 Pro"));
        assert_eq!(models[1].context_tokens, Some(1_048_576));
        assert_eq!(models[1].max_output_tokens, Some(65_536));

        assert!(parse_openai_models(native).is_empty());
        assert!(parse_gemini_models("nonsense").is_empty());
    }

    #[test]
    fn provider_url_requires_https_for_remote_hosts() {
        assert_eq!(normalize_base_url(" https://api.deepseek.com/ ").unwrap(), "https://api.deepseek.com");
        assert_eq!(normalize_base_url("https://api.openai.com/v1/").unwrap(), "https://api.openai.com/v1");
        assert!(matches!(normalize_base_url("http://api.example.com/v1"), Err(AiError::InvalidBaseUrl(_))));
        assert!(matches!(normalize_base_url("http://192.168.1.10:8000/v1"), Err(AiError::InvalidBaseUrl(_))));
        assert_eq!(normalize_base_url("http://localhost:11434").unwrap(), "http://localhost:11434");
        assert_eq!(normalize_base_url("http://127.0.0.1:11434").unwrap(), "http://127.0.0.1:11434");
        assert_eq!(normalize_base_url("http://[::1]:11434").unwrap(), "http://[::1]:11434");
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
    fn redirects_must_stay_on_the_same_origin() {
        let url = |value: &str| reqwest::Url::parse(value).unwrap();
        assert!(is_same_origin(&url("https://api.anthropic.com/v1/messages"), &url("https://API.anthropic.com/other")));
        assert!(is_same_origin(&url("https://api.example.com/a"), &url("https://api.example.com:443/b")));
        assert!(!is_same_origin(&url("https://api.example.com/a"), &url("https://evil.example.net/a")));
        assert!(!is_same_origin(&url("https://api.example.com/a"), &url("http://api.example.com/a")));
        assert!(!is_same_origin(&url("http://127.0.0.1:1000/a"), &url("http://127.0.0.1:2000/a")));
    }

    #[test]
    fn debug_output_never_contains_the_api_key() {
        let config = AiConfig { api_key: "sk-super-secret".into(), ..Default::default() };
        let formatted = format!("{config:?}");
        assert!(!formatted.contains("sk-super-secret"), "{formatted}");
        assert!(formatted.contains("<redacted>"), "{formatted}");
        let client = DeepSeekClient::new(config).expect("client");
        let formatted = format!("{client:?}");
        assert!(!formatted.contains("sk-super-secret"), "{formatted}");
        assert!(format!("{:?}", AiConfig::default()).contains("<empty>"));
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

    #[test]
    fn openai_compatible_image_is_a_data_url_part() {
        let message = ChatMessage::user_with_image("Describe", "image/jpeg", "AAAA");
        let wire = OpenAiWireMessage::from(&message);
        let content = serde_json::to_value(&wire.content).expect("serialize");
        let parts = content.as_array().expect("an image message has content parts");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "Describe");
        assert_eq!(parts[1]["type"], "image_url");
        assert!(parts[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn a_text_only_message_stays_a_plain_string() {
        let message = ChatMessage::user("hi");
        let wire = OpenAiWireMessage::from(&message);
        assert!(serde_json::to_value(&wire.content).expect("serialize").is_string());
    }

    #[test]
    fn gemini_uses_the_openai_compatible_chat_endpoint() {
        let config = AiConfig { provider: ProviderKind::Gemini, api_key: "test".into(), ..Default::default() };
        let client = DeepSeekClient::new(config).expect("client");
        let endpoint = client.endpoint();
        assert!(endpoint.ends_with("/openai/chat/completions"), "{endpoint}");
    }

    #[test]
    fn fuzz_ai_responses_handles_model_lists_streams_and_corrupt_inputs() {
        fuzz_ai_responses(br#"{"data":[{"id":"deepseek-chat"}]}"#);
        fuzz_ai_responses(
            b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Merhaba [s.1: \\\"ornek\\\"]\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        fuzz_ai_responses(&[0xff, 0x00, b'{', b'"', b'e', b'r', b'r']);
    }
}

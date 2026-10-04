//! Tests for the DeepSeek client against a local mock server, plus prompt
//! build/chunk/retrieval checks. No real network access is used.

use aicore::prompts::{
    ask_prompt, calc_action_prompt, chunk_text, doc_action_prompt, doc_chat_focus, doc_chat_messages, metadata_prompt,
    parse_formula_reply, parse_metadata_reply, select_relevant_pages, summarize_prompt, summarize_prompt_with_budget,
    summarize_selection_prompt, translate_page_prompt, writer_action_prompt, DocChatMessage, Plan, SummaryLength,
    SummaryOptions, SummaryStyle, TranslateOptions,
};
use aicore::{
    provider_notes, AiConfig, AiError, CancelToken, ChatMessage, ChatOptions, DeepSeekClient, ProviderCapabilities,
    ProviderKind,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// Starts a mock HTTP server; `responder` returns the raw body for a request.
fn mock_server(
    status: u16,
    content_type: &'static str,
    body: String,
    chunked: bool,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = read_request(&mut stream);
            if chunked {
                let header = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes());
                for line in body.split_inclusive('\n') {
                    let frame = format!("{:x}\r\n{}\r\n", line.len(), line);
                    if stream.write_all(frame.as_bytes()).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                    thread::sleep(std::time::Duration::from_millis(5));
                }
                let _ = stream.write_all(b"0\r\n\r\n");
                let _ = stream.flush();
            } else {
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        }
    });
    (format!("http://{address}"), handle)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 4096];
    let mut request = String::new();
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    while let Ok(read) = stream.read(&mut buffer) {
        if read == 0 {
            break;
        }
        request.push_str(&String::from_utf8_lossy(&buffer[..read]));
        if request.contains("\r\n\r\n") {
            // Body may still be pending; the mock does not need it.
            break;
        }
    }
    request
}

/// Reads a full request (headers and Content-Length body). Required by the
/// privacy test, which inspects what the client actually sent.
fn read_request_with_body(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 8192];
    let mut request: Vec<u8> = Vec::new();
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                request.extend_from_slice(&buffer[..read]);
                let header_end =
                    request.windows(4).position(|window| window == b"\r\n\r\n").map(|position| position + 4);
                if let Some(header_end) = header_end {
                    let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:")?.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= header_end + length {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&request).to_string()
}

/// Like `mock_server`, but also hands the received request back to the test.
fn mock_server_capturing(
    body: String,
    content_type: &'static str,
    chunked: bool,
) -> (String, Arc<Mutex<String>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    let captured: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let captured_clone = captured.clone();
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            *captured_clone.lock().unwrap() = read_request_with_body(&mut stream);
            if chunked {
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes());
                for line in body.split_inclusive('\n') {
                    let frame = format!("{:x}\r\n{}\r\n", line.len(), line);
                    if stream.write_all(frame.as_bytes()).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                    thread::sleep(std::time::Duration::from_millis(5));
                }
                let _ = stream.write_all(b"0\r\n\r\n");
                let _ = stream.flush();
            } else {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        }
    });
    (format!("http://{address}"), captured, handle)
}

fn client_for(base_url: String) -> DeepSeekClient {
    DeepSeekClient::new(AiConfig { api_key: "test-key".into(), base_url, ..Default::default() }).expect("client")
}

fn provider_client(base_url: String, provider: ProviderKind, api_key: &str) -> DeepSeekClient {
    DeepSeekClient::new(AiConfig {
        api_key: api_key.into(),
        base_url,
        provider,
        model: match provider {
            ProviderKind::Ollama => "llama3.2".into(),
            _ => "test-model".into(),
        },
        ..Default::default()
    })
    .expect("client")
}

#[tokio::test]
async fn chat_reads_a_normal_completion() {
    let body = r#"{"choices":[{"message":{"role":"assistant","content":"  1200 EUR  "}}]}"#.to_string();
    let (url, handle) = mock_server(200, "application/json", body, false);
    let client = client_for(url);
    let reply = client.chat(&[ChatMessage::user("total?")], ChatOptions::default()).await.expect("completion");
    assert_eq!(reply.trim(), "1200 EUR");
    handle.join().unwrap();
}

#[tokio::test]
async fn chat_stream_assembles_server_sent_events() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n",
        "data: [DONE]\n\n"
    )
    .to_string();
    let (url, handle) = mock_server(200, "text/event-stream", body, true);
    let client = client_for(url);
    let cancel = CancelToken::new();
    let mut deltas = String::new();
    let full = client
        .chat_stream(
            &[ChatMessage::user("hi")],
            ChatOptions::default(),
            &cancel,
            &mut |delta| deltas.push_str(delta),
            &mut |_| {},
        )
        .await
        .expect("stream");
    assert_eq!(full, "Hello world");
    assert_eq!(deltas, "Hello world");
    handle.join().unwrap();
}

#[tokio::test]
async fn http_errors_map_to_friendly_codes() {
    for (status, expected) in [(401, "invalid_api_key"), (402, "insufficient_balance"), (429, "rate_limited")] {
        let body = format!(r#"{{"error":{{"message":"nope","code":"{status}"}}}}"#);
        let (url, handle) = mock_server(status, "application/json", body, false);
        let client = client_for(url);
        let error = client.chat(&[ChatMessage::user("x")], ChatOptions::default()).await.expect_err("should fail");
        let code = match error {
            AiError::InvalidApiKey(_) => "invalid_api_key",
            AiError::InsufficientBalance(_) => "insufficient_balance",
            AiError::RateLimited(_) => "rate_limited",
            other => panic!("unexpected error {other:?}"),
        };
        assert_eq!(code, expected);
        handle.join().unwrap();
    }
}

#[tokio::test]
async fn missing_api_key_is_rejected_before_any_request() {
    let config = AiConfig { api_key: String::new(), ..Default::default() };
    assert!(matches!(DeepSeekClient::new(config), Err(AiError::MissingApiKey)));
}

#[tokio::test]
async fn cancellation_stops_the_stream() {
    let body = (0..50)
        .map(|index| format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{index}\"}}}}]}}\n\n"))
        .collect::<String>();
    let (url, handle) = mock_server(200, "text/event-stream", body, true);
    let client = client_for(url);
    let cancel = CancelToken::new();
    cancel.cancel();
    let result =
        client.chat_stream(&[ChatMessage::user("hi")], ChatOptions::default(), &cancel, &mut |_| {}, &mut |_| {}).await;
    assert!(matches!(result, Err(AiError::Cancelled)));
    handle.join().unwrap();
}

#[tokio::test]
async fn reasoning_only_streams_return_the_thinking_trace() {
    // Thinking mode can consume the whole token budget before any answer
    // text arrives; the client must not fail with an empty response.
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Let me \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think about this.\"}}]}\n\n",
        "data: [DONE]\n\n"
    )
    .to_string();
    let (url, handle) = mock_server(200, "text/event-stream", body, true);
    let client = client_for(url);
    let cancel = CancelToken::new();
    let mut reasoning = String::new();
    let mut answer = String::new();
    let full = client
        .chat_stream(
            &[ChatMessage::user("hi")],
            ChatOptions::default(),
            &cancel,
            &mut |delta| answer.push_str(delta),
            &mut |delta| reasoning.push_str(delta),
        )
        .await
        .expect("reasoning-only stream must succeed");
    assert_eq!(full, "Let me think about this.");
    assert_eq!(reasoning, "Let me think about this.");
    assert!(answer.is_empty());
    handle.join().unwrap();
}

#[tokio::test]
async fn thinking_deltas_are_reported_separately_from_the_answer() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"checking numbers...\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"Total is 1200 EUR.\"}}]}\n\n",
        "data: [DONE]\n\n"
    )
    .to_string();
    let (url, handle) = mock_server(200, "text/event-stream", body, true);
    let client = client_for(url);
    let cancel = CancelToken::new();
    let mut reasoning = String::new();
    let mut answer = String::new();
    let full = client
        .chat_stream(
            &[ChatMessage::user("total?")],
            ChatOptions::default(),
            &cancel,
            &mut |delta| answer.push_str(delta),
            &mut |delta| reasoning.push_str(delta),
        )
        .await
        .expect("stream");
    assert_eq!(full, "Total is 1200 EUR.");
    assert_eq!(answer, "Total is 1200 EUR.");
    assert_eq!(reasoning, "checking numbers...");
    handle.join().unwrap();
}

#[test]
fn thinking_parameters_only_apply_to_v4_models() {
    assert!(aicore::supports_thinking("deepseek-v4-flash"));
    assert!(aicore::supports_thinking("deepseek-v4-pro"));
    assert!(!aicore::supports_thinking("deepseek-chat"));
    assert!(!aicore::supports_thinking("llama3.1:8b"));
    let config = AiConfig { api_key: "k".into(), model: "deepseek-v4-flash".into(), ..Default::default() };
    assert!(config.thinking);
    assert_eq!(config.reasoning_effort, "high");
}

#[tokio::test]
async fn thinking_fields_are_sent_to_v4_models_and_omitted_otherwise() {
    // The mock echoes the received request body via the reasoning channel, so
    // the test can inspect what the client actually sent.
    let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured_clone = captured.clone();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let request = read_request(&mut stream);
            let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            *captured_clone.lock().unwrap() = body;
            let reply = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n";
            let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            for line in reply.split_inclusive('\n') {
                let frame = format!("{:x}\r\n{}\r\n", line.len(), line);
                let _ = stream.write_all(frame.as_bytes());
            }
            let _ = stream.write_all(b"0\r\n\r\n");
            let _ = stream.flush();
        }
    });

    let client = client_for(format!("http://{address}"));
    let cancel = CancelToken::new();
    client
        .chat_stream(&[ChatMessage::user("hi")], ChatOptions::default(), &cancel, &mut |_| {}, &mut |_| {})
        .await
        .expect("v4 stream");
    handle.join().unwrap();
    let body = captured.lock().unwrap().clone();
    assert!(body.contains("\"thinking\":{\"type\":\"enabled\"}"), "thinking missing in {body}");
    assert!(body.contains("\"reasoning_effort\":\"high\""), "effort missing in {body}");

    // A legacy model must not receive the V4-only fields.
    let captured2 = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured2_clone = captured2.clone();
    let listener2 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address2 = listener2.local_addr().unwrap();
    let handle2 = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener2.accept() {
            let request = read_request(&mut stream);
            let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
            *captured2_clone.lock().unwrap() = body;
            let reply = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n";
            let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            for line in reply.split_inclusive('\n') {
                let frame = format!("{:x}\r\n{}\r\n", line.len(), line);
                let _ = stream.write_all(frame.as_bytes());
            }
            let _ = stream.write_all(b"0\r\n\r\n");
            let _ = stream.flush();
        }
    });
    let legacy = DeepSeekClient::new(AiConfig {
        api_key: "k".into(),
        base_url: format!("http://{address2}"),
        model: "deepseek-chat".into(),
        ..Default::default()
    })
    .unwrap();
    legacy
        .chat_stream(&[ChatMessage::user("hi")], ChatOptions::default(), &cancel, &mut |_| {}, &mut |_| {})
        .await
        .expect("legacy stream");
    handle2.join().unwrap();
    let body2 = captured2.lock().unwrap().clone();
    assert!(!body2.contains("thinking"), "legacy model must not receive thinking: {body2}");
    assert!(!body2.contains("reasoning_effort"), "legacy model must not receive effort: {body2}");
}

#[test]
fn chunking_splits_on_paragraphs_and_keeps_order() {
    let paragraph = "Sentence about invoices and payments. ".repeat(60); // ~2.3k chars
    let text = (0..30).map(|index| format!("{paragraph} [para {index}]")).collect::<Vec<_>>().join("\n\n");
    let chunks = chunk_text(&text, 8_000);
    assert!(chunks.len() > 1, "expected chunking, got {}", chunks.len());
    assert!(chunks.iter().all(|chunk| chunk.len() <= 8_000));
    let rebuilt = chunks.join(" ");
    assert!(rebuilt.contains("[para 0]"));
    assert!(rebuilt.contains("[para 29]"));
}

#[test]
fn summarize_prompt_switches_to_map_reduce_for_long_documents() {
    let short = summarize_prompt("Short text", &SummaryOptions::default());
    assert!(matches!(short, Plan::Single { .. }));

    let long_text = "word ".repeat(20_000); // ~100k chars
    let plan = summarize_prompt(&long_text, &SummaryOptions::default());
    match plan {
        Plan::MapReduce { chunks, reduce } => {
            assert!(chunks.len() >= 3);
            assert!(reduce.contains("Summarize the document text below"));
        }
        Plan::Single { .. } => panic!("expected map/reduce for long input"),
    }
}

#[test]
fn context_budget_scales_the_request_size() {
    // 200k tokens (default) must fit far more text than the old 40k chars,
    // and the 1M maximum must not be chunked for a 2M-character document.
    let default_budget = aicore::chunk_chars_for_context(200_000);
    assert!(default_budget > 300_000, "default budget too small: {default_budget}");
    let max_budget = aicore::chunk_chars_for_context(1_000_000);
    assert_eq!(max_budget, 2_000_000);
    assert_eq!(aicore::chunk_chars_for_context(9_999_999), 2_000_000, "clamped to the 1M context");

    let config = AiConfig { api_key: "k".into(), context_tokens: 1_000_000, ..Default::default() };
    assert_eq!(config.chunk_chars(), 2_000_000);

    // A 1.2M-character document fits into a single request at 1M tokens...
    let long_text = "kelime ".repeat(200_000); // 1.4M chars
    let plan = summarize_prompt_with_budget(&long_text, &SummaryOptions::default(), config.chunk_chars());
    assert!(matches!(plan, Plan::Single { .. }));
    // ...while the same text is chunked with a small budget.
    let chunked = summarize_prompt_with_budget(&long_text, &SummaryOptions::default(), 40_000);
    match chunked {
        Plan::MapReduce { chunks, .. } => assert!(chunks.len() > 20),
        Plan::Single { .. } => panic!("expected chunking with a 40k budget"),
    }
}

#[test]
fn output_tokens_are_clamped_to_the_model_limit() {
    assert_eq!(aicore::clamp_output_tokens(100), 256);
    assert_eq!(aicore::clamp_output_tokens(8_192), 8_192);
    assert_eq!(aicore::clamp_output_tokens(384_000), 384_000);
    assert_eq!(aicore::clamp_output_tokens(1_000_000), aicore::MAX_OUTPUT_TOKENS);
    assert_eq!(aicore::MAX_OUTPUT_TOKENS, 384_000);
    assert_eq!(aicore::MAX_CONTEXT_TOKENS, 1_000_000);
}

#[test]
fn summarize_prompt_includes_style_and_language() {
    let plan = summarize_prompt(
        "text",
        &SummaryOptions {
            language: "Turkish".into(),
            length: SummaryLength::Short,
            style: SummaryStyle::Bullets,
            focus: "payment terms".into(),
        },
    );
    let Plan::Single { messages } = plan else {
        panic!("expected single request");
    };
    let user_message = &messages[1].content;
    assert!(user_message.contains("Reply in Turkish."));
    assert!(user_message.contains("3-5 sentences"));
    assert!(user_message.contains("bullet points"));
    assert!(user_message.contains("payment terms"));
}

#[test]
fn translation_prompt_keeps_markers_and_optional_bilingual_output() {
    let messages = translate_page_prompt(
        "Page 3",
        "Hello world",
        &TranslateOptions { target_language: "tr".into(), bilingual: true },
    );
    assert_eq!(messages[0].role, "system");
    let user = &messages[1].content;
    assert!(user.contains("into tr"));
    assert!(user.contains("Page marker: Page 3"));
    assert!(user.contains("Hello world"));
    assert!(user.contains("Original"));
}

#[test]
fn relevance_retrieval_prefers_matching_pages() {
    let pages = vec![
        (1, "Introduction and table of contents".to_string()),
        (2, "The total amount due is 1200 EUR, payable in 30 days.".to_string()),
        (3, "Appendix with addresses".to_string()),
        (4, "Payment terms: bank transfer to IBAN DE00.".to_string()),
    ];
    let selected = select_relevant_pages(&pages, "payment terms and total amount", 10_000);
    let page_numbers: Vec<u32> = selected.iter().map(|(page, _)| *page).collect();
    assert!(page_numbers.contains(&2));
    assert!(page_numbers.contains(&4));
    assert!(!page_numbers.contains(&3));
    // Results stay in document order.
    let mut sorted = page_numbers.clone();
    sorted.sort_unstable();
    assert_eq!(page_numbers, sorted);
}

#[test]
fn metadata_reply_is_parsed_from_json_with_chatter() {
    let reply = "Sure!\n```json\n{\"title\":\"Invoice 2026-01\",\"author\":\"Acme\",\"subject\":\"Billing\",\"keywords\":[\"invoice\",\"2026\",\"\"]}\n```";
    let parsed = parse_metadata_reply(reply).expect("parsed");
    assert_eq!(parsed.title, "Invoice 2026-01");
    assert_eq!(parsed.author, "Acme");
    assert_eq!(parsed.keywords, vec!["invoice", "2026"]);
    assert!(parse_metadata_reply("no json here").is_none());
}

#[test]
fn ask_and_metadata_prompts_are_bounded() {
    let long_text = "x".repeat(50_000);
    let messages = metadata_prompt(&long_text);
    assert!(messages[1].content.len() < 9_000, "metadata excerpt must be small");
    let ask = ask_prompt("[page 1]\nsome content", "What is the total?");
    assert!(ask[1].content.contains("What is the total?"));
    assert!(ask[0].content.contains("Cite the page numbers"));
}

// ---------------------------------------------------------------------------
// Provider abstraction
// ---------------------------------------------------------------------------

#[test]
fn provider_kinds_parse_round_trip_and_default_to_deepseek() {
    for kind in [
        ProviderKind::DeepSeek,
        ProviderKind::OpenAiCompatible,
        ProviderKind::Ollama,
        ProviderKind::Gemini,
        ProviderKind::Custom,
    ] {
        assert_eq!(ProviderKind::parse(kind.as_str()), Some(kind));
        assert!(!kind.label().is_empty());
    }
    assert_eq!(ProviderKind::parse("OpenAI-Compatible"), Some(ProviderKind::OpenAiCompatible));
    assert_eq!(ProviderKind::parse("LOCAL"), Some(ProviderKind::Ollama));
    assert_eq!(ProviderKind::parse("nope"), None);
    assert_eq!(ProviderKind::default(), ProviderKind::DeepSeek);

    let config: AiConfig = serde_json::from_str(r#"{"apiKey":"k"}"#).expect("config");
    assert_eq!(config.provider, ProviderKind::DeepSeek);
    assert_eq!(config.embedding_model, None);

    assert_eq!(ProviderKind::Ollama.default_base_url(), "http://localhost:11434");
    assert_eq!(ProviderKind::Gemini.default_base_url(), "https://generativelanguage.googleapis.com/v1beta");
}

#[test]
fn provider_capability_table_matches_each_provider() {
    let deepseek = ProviderCapabilities::for_kind(ProviderKind::DeepSeek);
    assert!(deepseek.chat && deepseek.streaming && deepseek.structured_output && deepseek.vision);
    assert!(!deepseek.embeddings);

    let openai = ProviderCapabilities::for_kind(ProviderKind::OpenAiCompatible);
    assert!(openai.chat && openai.streaming && openai.embeddings && openai.vision && openai.structured_output);

    let ollama = ProviderCapabilities::for_kind(ProviderKind::Ollama);
    assert!(ollama.chat && ollama.streaming && ollama.embeddings && ollama.vision);
    assert!(ollama.structured_output);

    let gemini = ProviderCapabilities::for_kind(ProviderKind::Gemini);
    assert!(gemini.chat && gemini.streaming && gemini.embeddings && gemini.vision && gemini.structured_output);

    let custom = ProviderCapabilities::for_kind(ProviderKind::Custom);
    assert!(custom.chat && custom.streaming);
    assert!(!custom.embeddings && !custom.vision && !custom.structured_output);

    let client = provider_client("http://127.0.0.1:1".into(), ProviderKind::Ollama, "");
    assert_eq!(client.provider(), ProviderKind::Ollama);
    assert_eq!(client.capabilities(), ollama);
}

#[test]
fn ollama_is_configured_without_an_api_key() {
    let config = AiConfig { api_key: String::new(), provider: ProviderKind::Ollama, ..Default::default() };
    assert!(config.is_configured());
    assert!(DeepSeekClient::new(config).is_ok());

    let cloud = AiConfig { api_key: String::new(), provider: ProviderKind::Gemini, ..Default::default() };
    assert!(!cloud.is_configured());
    assert!(matches!(DeepSeekClient::new(cloud), Err(AiError::MissingApiKey)));
}

#[test]
fn provider_notes_state_where_the_text_goes() {
    let note = provider_notes(ProviderKind::Ollama).to_lowercase();
    assert!(note.contains("never leaves"), "{note}");
    assert!(note.contains("computer"), "{note}");
    for kind in [ProviderKind::DeepSeek, ProviderKind::OpenAiCompatible, ProviderKind::Gemini, ProviderKind::Custom] {
        assert!(!provider_notes(kind).is_empty());
    }
}

#[tokio::test]
async fn each_provider_posts_to_its_own_endpoint() {
    // DeepSeek / OpenAI-compatible / Custom: {base}/chat/completions, and the
    // base may or may not already end with /v1.
    for (provider, base_suffix, expected) in [
        (ProviderKind::DeepSeek, "", "/chat/completions"),
        (ProviderKind::OpenAiCompatible, "", "/chat/completions"),
        (ProviderKind::OpenAiCompatible, "/v1", "/v1/chat/completions"),
        (ProviderKind::Custom, "", "/chat/completions"),
    ] {
        let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string();
        let (url, captured, handle) = mock_server_capturing(body, "application/json", false);
        let client = provider_client(format!("{url}{base_suffix}"), provider, "k");
        client.chat(&[ChatMessage::user("x")], ChatOptions::default()).await.expect("chat");
        handle.join().unwrap();
        let request = captured.lock().unwrap().clone();
        assert!(request.starts_with(&format!("POST {expected} ")), "{provider:?} sent {request}");
    }

    // Gemini: {base}/openai/chat/completions
    let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string();
    let (url, captured, handle) = mock_server_capturing(body, "application/json", false);
    provider_client(url, ProviderKind::Gemini, "k")
        .chat(&[ChatMessage::user("x")], ChatOptions::default())
        .await
        .expect("gemini chat");
    handle.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(request.starts_with("POST /openai/chat/completions "), "{request}");

    // Ollama: {base}/api/chat
    let body = r#"{"message":{"content":"ok"},"done":true}"#.to_string();
    let (url, captured, handle) = mock_server_capturing(body, "application/json", false);
    provider_client(url, ProviderKind::Ollama, "")
        .chat(&[ChatMessage::user("x")], ChatOptions::default())
        .await
        .expect("ollama chat");
    handle.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(request.starts_with("POST /api/chat "), "{request}");
}

#[tokio::test]
async fn ollama_chat_reads_the_message_field() {
    let body = r#"{"model":"llama3.2","message":{"role":"assistant","content":"  Merhaba  "},"done":true}"#.to_string();
    let (url, captured, handle) = mock_server_capturing(body, "application/json", false);
    let client = provider_client(url, ProviderKind::Ollama, "");
    let reply = client.chat(&[ChatMessage::user("selam")], ChatOptions::default()).await.expect("ollama chat");
    assert_eq!(reply.trim(), "Merhaba");
    handle.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(request.contains("\"stream\":false"), "{request}");
}

#[tokio::test]
async fn ollama_stream_assembles_ndjson_lines() {
    let body = concat!(
        "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"},\"done\":false}\n",
        "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\" from\"},\"done\":false}\n",
        "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\" Ollama\"},\"done\":false}\n",
        "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n"
    )
    .to_string();
    let (url, captured, handle) = mock_server_capturing(body, "application/x-ndjson", true);
    let client = provider_client(url, ProviderKind::Ollama, "");
    let cancel = CancelToken::new();
    let mut deltas = String::new();
    let full = client
        .chat_stream(
            &[ChatMessage::user("hi")],
            ChatOptions::default(),
            &cancel,
            &mut |delta| deltas.push_str(delta),
            &mut |_| {},
        )
        .await
        .expect("ollama stream");
    assert_eq!(full, "Hello from Ollama");
    assert_eq!(deltas, "Hello from Ollama");
    handle.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(request.starts_with("POST /api/chat "), "{request}");
    assert!(request.contains("\"stream\":true"), "{request}");
}

#[tokio::test]
async fn ollama_requests_never_include_the_api_key() {
    let body = r#"{"message":{"content":"ok"},"done":true}"#.to_string();
    let (url, captured, handle) = mock_server_capturing(body, "application/json", false);
    let client = provider_client(url, ProviderKind::Ollama, "sk-local-secret");
    client.chat(&[ChatMessage::user("gizli metin")], ChatOptions::default()).await.expect("ollama chat");
    handle.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(!request.contains("sk-local-secret"), "request leaked the key: {request}");
    assert!(!request.to_lowercase().contains("authorization"), "request carried an auth header: {request}");
    assert!(request.contains("gizli metin"), "body was not captured: {request}");
}

#[tokio::test]
async fn unreachable_provider_reports_a_friendly_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    drop(listener);
    let config = AiConfig {
        api_key: String::new(),
        provider: ProviderKind::Ollama,
        base_url: format!("http://{address}"),
        model: "llama3.2".into(),
        ..Default::default()
    };
    let client = DeepSeekClient::new(config).expect("client");
    let error = client.chat(&[ChatMessage::user("hi")], ChatOptions::default()).await.expect_err("must fail");
    match error {
        AiError::ProviderUnreachable(message) => {
            assert!(message.contains("could not reach"), "{message}");
            assert!(message.contains("Ollama"), "{message}");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// New prompt builders
// ---------------------------------------------------------------------------

#[test]
fn doc_chat_messages_carry_context_history_and_the_fallback_sentence() {
    let context = vec![(1, "Invoice total: 1200 EUR".to_string()), (4, "Payment due in 30 days".to_string())];
    let history =
        vec![DocChatMessage::user("What is the total?"), DocChatMessage::assistant("It is 1200 EUR [page 1].")];
    let messages = doc_chat_messages("When is payment due?", &context, &history);
    assert_eq!(messages[0].role, "system");
    assert!(messages[0].content.contains("I could not find this in the document."));
    assert!(messages.iter().any(|message| message.content.contains("[page 1]")));
    assert!(messages.iter().any(|message| message.content.contains("[page 4]")));
    assert!(messages.iter().any(|message| message.role == "assistant" && message.content.contains("1200 EUR")));
    assert_eq!(messages.last().unwrap().content, "When is payment due?");
}

#[test]
fn doc_chat_focus_selects_matching_pages_and_falls_back() {
    let pages = vec![
        (1, "Introduction and table of contents".to_string()),
        (2, "The total amount due is 1200 EUR.".to_string()),
        (3, "Appendix with addresses".to_string()),
        (4, "Payment terms: bank transfer to IBAN DE00.".to_string()),
        (5, "Signatures".to_string()),
    ];
    let selected = doc_chat_focus("payment terms and total amount", &pages, 2);
    let page_numbers: Vec<u32> = selected.iter().map(|(page, _)| *page).collect();
    assert_eq!(page_numbers, vec![2, 4]);
    assert!(selected.iter().all(|(_, text)| !text.starts_with("Appendix")));

    let fallback = doc_chat_focus("zzzzz qqqqq", &pages, 2);
    assert_eq!(fallback.iter().map(|(page, _)| *page).collect::<Vec<_>>(), vec![1, 2]);
}

#[test]
fn formula_replies_are_extracted_from_fences() {
    assert_eq!(parse_formula_reply("```formula\n=SUM(A1:A5)\n```").as_deref(), Some("=SUM(A1:A5)"));
    assert_eq!(parse_formula_reply("```\n=AVERAGE(B1:B9)\n```").as_deref(), Some("=AVERAGE(B1:B9)"));
    assert_eq!(parse_formula_reply("Use this:\n=IF(A1>0,\"ok\",\"no\")").as_deref(), Some("=IF(A1>0,\"ok\",\"no\")"));
    assert_eq!(parse_formula_reply("```formula\n  =ROUND(A1;2)  \n```").as_deref(), Some("=ROUND(A1;2)"));
    assert_eq!(parse_formula_reply("no formula here"), None);
    assert_eq!(parse_formula_reply("```formula\n\n```"), None);
}

#[test]
fn unknown_writer_action_is_rejected() {
    match writer_action_prompt("summarize the moon", "text") {
        Err(AiError::Unsupported(message)) => assert!(message.contains("summarize the moon")),
        other => panic!("unexpected result: {other:?}"),
    }
    for action in
        ["rewrite", "formalize", "simplify", "shorten", "expand", "grammar", "translate", "translate:de", "continue"]
    {
        assert!(writer_action_prompt(action, "text").is_ok(), "{action}");
    }
    let translated = writer_action_prompt("translate:de", "Merhaba").expect("translate");
    assert!(translated[1].content.contains("into de"));
}

#[test]
fn formula_actions_require_a_fenced_formula_block() {
    for action in ["suggest_formula", "repair_formula", "generate_formula"] {
        let messages = calc_action_prompt(action, "sum column A", "A1: 10").expect("action");
        assert!(messages[1].content.contains("```formula"), "{action}");
        assert!(messages[1].content.contains("A1: 10"), "{action}");
    }
    let explain = calc_action_prompt("explain_formula", "what does this do?", "=SUM(A1:A5)").expect("action");
    assert!(explain[1].content.contains("=SUM(A1:A5)"));
    assert!(matches!(calc_action_prompt("nope", "q", ""), Err(AiError::Unsupported(_))));
}

#[test]
fn document_actions_request_the_right_output_format() {
    let table = doc_action_prompt("extract_table", "a\tb").expect("table");
    assert!(table[1].content.contains("TSV"));
    let dates = doc_action_prompt("extract_dates", "on 2026-01-01").expect("dates");
    assert!(dates[1].content.contains("JSON array"));
    let names = doc_action_prompt("extract_names", "Acme GmbH").expect("names");
    assert!(names[1].content.contains("JSON array"));
    let items = doc_action_prompt("action_items", "send the invoice").expect("items");
    assert!(items[1].content.contains("JSON array"));
    assert!(matches!(doc_action_prompt("dance", "x"), Err(AiError::Unsupported(_))));
}

#[test]
fn selection_summary_prompt_uses_the_requested_language() {
    let messages = summarize_selection_prompt("Selected words", "Turkish");
    assert_eq!(messages.len(), 2);
    assert!(messages[1].content.contains("Summarize the selected text"));
    assert!(messages[1].content.contains("Reply in Turkish."));
    assert!(messages[1].content.contains("Selected words"));
    let auto = summarize_selection_prompt("Words", "auto");
    assert!(auto[1].content.contains("same language as the document"));
}

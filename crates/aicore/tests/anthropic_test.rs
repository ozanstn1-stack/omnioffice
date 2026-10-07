//! Anthropic (Claude) provider and Gemini model discovery against a loopback
//! HTTP server. No real network access is used.

use aicore::{AiConfig, AiError, CancelToken, ChatMessage, ChatOptions, DeepSeekClient, ProviderKind};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// Reads a full request (headers and Content-Length body).
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

struct Canned {
    status: u16,
    content_type: &'static str,
    body: String,
    chunked: bool,
}

fn canned(status: u16, content_type: &'static str, body: impl Into<String>, chunked: bool) -> Canned {
    Canned { status, content_type, body: body.into(), chunked }
}

/// Serves one canned response per connection, in order, and records every
/// request it receives. Chunked bodies are written line by line.
fn mock_sequence_server(responses: Vec<Canned>) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr");
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let handle = thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            captured_clone.lock().unwrap().push(read_request_with_body(&mut stream));
            let Canned { status, content_type, body, chunked } = response;
            if chunked {
                let header = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes());
                for line in body.split_inclusive('\n') {
                    let frame = format!("{:x}\r\n{}\r\n", line.len(), line);
                    if stream.write_all(frame.as_bytes()).is_err() {
                        break;
                    }
                    let _ = stream.flush();
                    thread::sleep(std::time::Duration::from_millis(2));
                }
                let _ = stream.write_all(b"0\r\n\r\n");
            } else {
                let reply = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
            let _ = stream.flush();
        }
    });
    (format!("http://{address}"), captured, handle)
}

fn claude_client(base_url: String) -> DeepSeekClient {
    DeepSeekClient::new(AiConfig {
        api_key: "sk-ant-secret".into(),
        base_url,
        provider: ProviderKind::Anthropic,
        model: "claude-opus-5-5".into(),
        ..Default::default()
    })
    .expect("client")
}

fn request_body(request: &str) -> serde_json::Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or("")).expect("json body")
}

#[tokio::test]
async fn chat_posts_to_v1_messages_with_api_key_headers() {
    let body = r#"{"id":"msg_1","type":"message","role":"assistant","stop_reason":"end_turn",
        "content":[{"type":"thinking","thinking":"hmm"},{"type":"text","text":"  ready  "}]}"#;
    let (url, captured, handle) = mock_sequence_server(vec![canned(200, "application/json", body, false)]);
    let reply = claude_client(url)
        .chat(
            &[ChatMessage::system("Be terse."), ChatMessage::user("ping")],
            ChatOptions { temperature: Some(0.5), max_tokens: Some(512) },
        )
        .await
        .expect("chat");
    assert_eq!(reply.trim(), "ready");
    handle.join().unwrap();

    let request = captured.lock().unwrap()[0].clone();
    let lowered = request.to_lowercase();
    assert!(request.starts_with("POST /v1/messages "), "{request}");
    assert!(lowered.contains("x-api-key: sk-ant-secret"), "{request}");
    assert!(lowered.contains("anthropic-version: 2023-06-01"), "{request}");
    assert!(lowered.contains("content-type: application/json"), "{request}");
    assert!(!lowered.contains("authorization"), "no bearer auth for Anthropic: {request}");

    let body = request_body(&request);
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["max_tokens"], 512);
    assert_eq!(body["system"], "Be terse.");
    assert_eq!(body["stream"], false);
    assert_eq!(body["messages"], serde_json::json!([{"role": "user", "content": "ping"}]));
    for field in ["temperature", "top_p", "top_k", "thinking"] {
        assert!(body.get(field).is_none(), "{field} must not be sent: {body}");
    }
}

#[tokio::test]
async fn stream_reports_text_and_thinking_chunks() {
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"checking\"}}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\" Claude\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n"
    );
    let (url, captured, handle) = mock_sequence_server(vec![canned(200, "text/event-stream", body, true)]);
    let cancel = CancelToken::new();
    let mut answer = String::new();
    let mut reasoning = String::new();
    let full = claude_client(url)
        .chat_stream(
            &[ChatMessage::system("sys"), ChatMessage::user("hi")],
            ChatOptions::default(),
            &cancel,
            &mut |delta| answer.push_str(delta),
            &mut |delta| reasoning.push_str(delta),
        )
        .await
        .expect("stream");
    handle.join().unwrap();
    assert_eq!(full, "Hello Claude");
    assert_eq!(answer, "Hello Claude");
    assert_eq!(reasoning, "checking");
    let request = captured.lock().unwrap()[0].clone();
    assert!(request.starts_with("POST /v1/messages "), "{request}");
    assert_eq!(request_body(&request)["stream"], true);
}

#[tokio::test]
async fn stream_error_event_fails_and_cancellation_stops() {
    let body = concat!(
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"par\"}}\n\n",
        "event: error\n",
        "data: {\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\",\"message\":\"slow down\"}}\n\n"
    );
    let (url, _captured, handle) = mock_sequence_server(vec![canned(200, "text/event-stream", body, true)]);
    let error = claude_client(url)
        .chat_stream(&[ChatMessage::user("hi")], ChatOptions::default(), &CancelToken::new(), &mut |_| {}, &mut |_| {})
        .await
        .expect_err("error event must fail the call");
    assert!(matches!(error, AiError::RateLimited(_)), "{error:?}");
    handle.join().unwrap();

    let (url, _captured, handle) = mock_sequence_server(vec![canned(200, "text/event-stream", body, true)]);
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = claude_client(url)
        .chat_stream(&[ChatMessage::user("hi")], ChatOptions::default(), &cancel, &mut |_| {}, &mut |_| {})
        .await;
    assert!(matches!(result, Err(AiError::Cancelled)));
    handle.join().unwrap();
}

#[tokio::test]
async fn http_errors_use_the_error_body_type() {
    for (status, kind, expected) in [
        (401, "authentication_error", "invalid_api_key"),
        (403, "permission_error", "invalid_api_key"),
        (429, "rate_limit_error", "rate_limited"),
        (529, "overloaded_error", "server"),
        (400, "invalid_request_error", "server"),
    ] {
        let body = format!(r#"{{"type":"error","error":{{"type":"{kind}","message":"nope"}}}}"#);
        let (url, _captured, handle) = mock_sequence_server(vec![canned(status, "application/json", body, false)]);
        let error = claude_client(url).chat(&[ChatMessage::user("x")], ChatOptions::default()).await.unwrap_err();
        let code = match error {
            AiError::InvalidApiKey(provider) => {
                assert_eq!(provider, "Anthropic (Claude)");
                "invalid_api_key"
            }
            AiError::RateLimited(_) => "rate_limited",
            AiError::Server(_, _) => "server",
            other => panic!("unexpected error {other:?}"),
        };
        assert_eq!(code, expected, "{status} {kind}");
        handle.join().unwrap();
    }
}

#[tokio::test]
async fn refusal_is_a_clear_error() {
    let body = r#"{"stop_reason":"refusal","content":[{"type":"text","text":"no"}]}"#;
    let (url, _captured, handle) = mock_sequence_server(vec![canned(200, "application/json", body, false)]);
    let error = claude_client(url).chat(&[ChatMessage::user("x")], ChatOptions::default()).await.unwrap_err();
    assert!(error.to_string().contains("declined this request"), "{error}");
    handle.join().unwrap();
}

#[tokio::test]
async fn test_connection_and_configuration_work_for_anthropic() {
    let body = r#"{"stop_reason":"end_turn","content":[{"type":"text","text":"ready"}]}"#;
    let (url, _captured, handle) = mock_sequence_server(vec![canned(200, "application/json", body, false)]);
    assert_eq!(claude_client(url).test_connection().await.expect("test"), "ready");
    handle.join().unwrap();

    let without_key = AiConfig { provider: ProviderKind::Anthropic, api_key: String::new(), ..Default::default() };
    assert!(!without_key.is_configured());
    assert!(matches!(DeepSeekClient::new(without_key), Err(AiError::MissingApiKey)));
    let with_key = AiConfig { provider: ProviderKind::Anthropic, api_key: "sk-ant-x".into(), ..Default::default() };
    assert!(with_key.is_configured());
}

#[tokio::test]
async fn model_discovery_follows_pagination() {
    let first = r#"{"data":[{"type":"model","id":"claude-sonnet-5-5","display_name":"Claude Sonnet 5.5"}],
        "has_more":true,"first_id":"claude-sonnet-5-5","last_id":"claude-sonnet-5-5"}"#;
    let second = r#"{"data":[{"type":"model","id":"claude-opus-5-5","display_name":"Claude Opus 5.5"}],
        "has_more":false,"first_id":"claude-opus-5-5","last_id":"claude-opus-5-5"}"#;
    let (url, captured, handle) = mock_sequence_server(vec![
        canned(200, "application/json", first, false),
        canned(200, "application/json", second, false),
    ]);
    let models = claude_client(url).discover_models().await.expect("models");
    handle.join().unwrap();
    let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, ["claude-opus-5-5", "claude-sonnet-5-5"]);
    assert_eq!(models[0].label.as_deref(), Some("Claude Opus 5.5"));

    let requests = captured.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET /v1/models?limit=100 "), "{}", requests[0]);
    assert!(requests[1].starts_with("GET /v1/models?limit=100&after_id=claude-sonnet-5-5 "), "{}", requests[1]);
    for request in &requests {
        let lowered = request.to_lowercase();
        assert!(lowered.contains("x-api-key: sk-ant-secret"), "{request}");
        assert!(lowered.contains("anthropic-version: 2023-06-01"), "{request}");
        assert!(!lowered.contains("authorization"), "{request}");
    }
}

#[tokio::test]
async fn model_discovery_stops_at_the_page_cap() {
    // A server that always claims there is another page must not loop forever.
    let pages: Vec<Canned> = (0..10)
        .map(|index| {
            let body =
                format!(r#"{{"data":[{{"id":"claude-m{index}"}}],"has_more":true,"last_id":"claude-m{index}"}}"#);
            canned(200, "application/json", body, false)
        })
        .collect();
    let (url, captured, handle) = mock_sequence_server(pages);
    let models = claude_client(url).discover_models().await.expect("models");
    handle.join().unwrap();
    assert_eq!(models.len(), 10);
    assert_eq!(captured.lock().unwrap().len(), 10);
}

#[tokio::test]
async fn gemini_discovery_uses_the_openai_compatible_listing() {
    let body = r#"{"object":"list","data":[{"id":"models/gemini-2.5-flash","object":"model"},{"id":"models/gemini-2.0-flash","object":"model"}]}"#;
    let (url, captured, handle) = mock_sequence_server(vec![canned(200, "application/json", body, false)]);
    let client = DeepSeekClient::new(AiConfig {
        api_key: "gem-key".into(),
        base_url: url,
        provider: ProviderKind::Gemini,
        model: "gemini-2.0-flash".into(),
        ..Default::default()
    })
    .expect("client");
    let models = client.discover_models().await.expect("models");
    handle.join().unwrap();
    let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, ["gemini-2.0-flash", "gemini-2.5-flash"]);
    let request = captured.lock().unwrap()[0].clone();
    assert!(request.starts_with("GET /openai/models "), "{request}");
    assert!(request.to_lowercase().contains("authorization: bearer gem-key"), "{request}");
}

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

use hermes_agent_core::{
    execute_provider_request, execute_provider_stream, ApiMode, Message, ProviderErrorClass,
    ProviderHttpOptions, ProviderRequestOptions, ProviderRouting,
};
use serde_json::{json, Value};

fn routing(base_url: String) -> ProviderRouting {
    let mut extra_headers = BTreeMap::new();
    extra_headers.insert("x-hermes-test".to_string(), "yes".to_string());
    ProviderRouting {
        provider: "mock-openai".to_string(),
        model: "mock-model".to_string(),
        base_url: Some(base_url),
        api_mode: ApiMode::ChatCompletions,
        extra_headers,
        provider_options: None,
    }
}

#[derive(Debug)]
struct CapturedRequest {
    head: String,
    body: Value,
}

fn mock_server(
    response_status: u16,
    response_body: Value,
) -> (String, mpsc::Receiver<CapturedRequest>) {
    mock_raw_server(
        response_status,
        "application/json",
        serde_json::to_string(&response_body).expect("response body serializes"),
    )
}

fn mock_raw_server(
    response_status: u16,
    content_type: &'static str,
    response_body: String,
) -> (String, mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let (head, body) = read_http_request(&mut stream);
        tx.send(CapturedRequest {
            head,
            body: serde_json::from_slice(&body).expect("request body is JSON"),
        })
        .expect("send captured request");
        let reason = if response_status == 200 { "OK" } else { "ERR" };
        write!(
            stream,
            "HTTP/1.1 {response_status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write response");
    });
    (format!("http://{addr}/v1"), rx)
}

fn read_http_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).expect("read request");
        assert!(n > 0, "client closed before headers");
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_subslice(&buffer, b"\r\n\r\n") {
            break pos + 4;
        }
    };

    let head = String::from_utf8(buffer[..header_end].to_vec()).expect("headers are utf8");
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content length"))
        })
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let n = stream.read(&mut chunk).expect("read request body");
        assert!(n > 0, "client closed before body");
        buffer.extend_from_slice(&chunk[..n]);
    }
    (
        head,
        buffer[header_end..header_end + content_length].to_vec(),
    )
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn executes_chat_completions_request_against_mock_provider() {
    let (base_url, rx) = mock_server(
        200,
        json!({
            "choices": [{
                "message": {"content": "hello from rust"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 3}
        }),
    );

    let result = execute_provider_request(
        &[Message::user("hello")],
        &[],
        &routing(base_url),
        &ProviderRequestOptions::default(),
        &ProviderHttpOptions {
            api_key: Some("test-token".to_string()),
            timeout_secs: 5,
        },
    )
    .expect("provider request succeeds");

    assert_eq!(result.status, 200);
    assert_eq!(
        result.parsed.assistant.content.as_deref(),
        Some("hello from rust")
    );
    assert_eq!(result.parsed.usage.input_tokens, 4);

    let captured = rx.recv().expect("captured request");
    assert!(captured.head.starts_with("POST /v1/chat/completions "));
    assert!(captured
        .head
        .to_ascii_lowercase()
        .contains("authorization: bearer test-token"));
    assert!(captured
        .head
        .to_ascii_lowercase()
        .contains("x-hermes-test: yes"));
    assert_eq!(captured.body["model"], "mock-model");
    assert_eq!(captured.body["messages"][0]["content"], "hello");
    assert_eq!(captured.body["stream"], false);
}

#[test]
fn classifies_provider_http_errors() {
    let (base_url, _rx) = mock_server(
        429,
        json!({
            "error": {"message": "rate limit"}
        }),
    );

    let err = execute_provider_request(
        &[Message::user("hello")],
        &[],
        &routing(base_url),
        &ProviderRequestOptions::default(),
        &ProviderHttpOptions::default(),
    )
    .expect_err("provider returns 429");

    assert_eq!(err.status, Some(429));
    assert_eq!(err.class, ProviderErrorClass::RateLimit);
    assert!(err.message.contains("rate limit"));
}

#[test]
fn executes_streaming_chat_completions_request_against_mock_provider() {
    let (base_url, rx) = mock_raw_server(
        200,
        "text/event-stream",
        concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hel\",\"reasoning_content\":\"r1\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_string(),
    );

    let result = execute_provider_stream(
        &[Message::user("hello")],
        &[],
        &routing(base_url),
        &ProviderRequestOptions::default(),
        &ProviderHttpOptions::default(),
    )
    .expect("streaming provider request succeeds");

    assert_eq!(result.status, 200);
    assert_eq!(result.content, "hello");
    assert_eq!(result.reasoning, "r1");
    assert!(result.done);
    assert_eq!(result.deltas.len(), 3);

    let captured = rx.recv().expect("captured request");
    assert_eq!(captured.body["stream"], true);
    assert_eq!(captured.body["messages"][0]["content"], "hello");
}

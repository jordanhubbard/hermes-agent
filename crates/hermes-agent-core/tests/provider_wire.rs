//! Provider request/response wire-shape tests for hermes-1oa.4.

use std::collections::BTreeMap;

use hermes_agent_core::{
    build_provider_request, classify_provider_error, parse_provider_response, parse_stream_delta,
    ApiMode, Message, ProviderErrorClass, ProviderRequestOptions, ProviderRouting, ToolDefinition,
    ToolFunction,
};
use serde_json::json;

fn routing(api_mode: ApiMode) -> ProviderRouting {
    ProviderRouting {
        provider: "test".to_string(),
        model: "primary-model".to_string(),
        base_url: None,
        api_mode,
        extra_headers: BTreeMap::new(),
        provider_options: None,
    }
}

fn tool() -> ToolDefinition {
    ToolDefinition {
        kind: "function".to_string(),
        function: ToolFunction {
            name: "read_file".to_string(),
            description: Some("Read a file".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
    }
}

#[test]
fn chat_completions_request_preserves_tools_service_tier_and_fallback_model() {
    let request = build_provider_request(
        &[Message::user("read README.md")],
        &[tool()],
        &routing(ApiMode::ChatCompletions),
        &ProviderRequestOptions {
            stream: true,
            service_tier: Some("flex".to_string()),
            fallback_model: Some("fallback-model".to_string()),
        },
    );

    assert_eq!(request["model"], "fallback-model");
    assert_eq!(request["stream"], true);
    assert_eq!(request["service_tier"], "flex");
    assert_eq!(request["messages"][0]["role"], "user");
    assert_eq!(request["tools"][0]["function"]["name"], "read_file");
}

#[test]
fn anthropic_request_splits_system_and_rewrites_tools() {
    let request = build_provider_request(
        &[Message::system("system text"), Message::user("hello")],
        &[tool()],
        &routing(ApiMode::Anthropic),
        &ProviderRequestOptions::default(),
    );

    assert_eq!(request["system"], "system text");
    assert_eq!(request["messages"][0]["role"], "user");
    assert_eq!(request["tools"][0]["name"], "read_file");
    assert_eq!(request["tools"][0]["input_schema"]["type"], "object");
}

#[test]
fn chat_completions_response_parses_reasoning_tool_calls_and_usage() {
    let parsed = parse_provider_response(
        ApiMode::ChatCompletions,
        &json!({
            "choices": [{
                "message": {
                    "content": null,
                    "reasoning_content": "thinking",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "completion_tokens_details": {"reasoning_tokens": 2}
            }
        }),
    )
    .unwrap();

    assert_eq!(parsed.assistant.reasoning.as_deref(), Some("thinking"));
    assert_eq!(parsed.assistant.tool_calls[0].name, "read_file");
    assert_eq!(
        parsed.assistant.tool_calls[0].arguments["path"],
        "README.md"
    );
    assert_eq!(parsed.usage.input_tokens, 10);
    assert_eq!(parsed.usage.output_tokens, 5);
    assert_eq!(parsed.usage.reasoning_tokens, 2);
    assert_eq!(parsed.finish_reason.as_deref(), Some("tool_calls"));
}

#[test]
fn responses_response_parses_text_reasoning_and_function_call() {
    let parsed = parse_provider_response(
        ApiMode::Responses,
        &json!({
            "status": "completed",
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{"text": "reasoned"}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_2",
                    "name": "search_files",
                    "arguments": "{\"pattern\":\"foo\"}"
                },
                {
                    "type": "message",
                    "content": [{"type": "output_text", "text": "done"}]
                }
            ],
            "usage": {"input_tokens": 3, "output_tokens": 4}
        }),
    )
    .unwrap();

    assert_eq!(parsed.assistant.content.as_deref(), Some("done"));
    assert_eq!(parsed.assistant.reasoning.as_deref(), Some("reasoned"));
    assert_eq!(parsed.assistant.tool_calls[0].arguments["pattern"], "foo");
    assert_eq!(parsed.usage.input_tokens, 3);
}

#[test]
fn anthropic_response_parses_text_tool_use_usage_and_stop_reason() {
    let parsed = parse_provider_response(
        ApiMode::Anthropic,
        &json!({
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_use", "id": "toolu_1", "name": "read_file", "input": {"path": "README.md"}}
            ],
            "usage": {"input_tokens": 7, "output_tokens": 8},
            "stop_reason": "tool_use"
        }),
    )
    .unwrap();

    assert_eq!(parsed.assistant.content.as_deref(), Some("hello"));
    assert_eq!(parsed.assistant.tool_calls[0].id, "toolu_1");
    assert_eq!(parsed.usage.input_tokens, 7);
    assert_eq!(parsed.finish_reason.as_deref(), Some("tool_use"));
}

#[test]
fn streaming_delta_parsers_cover_text_reasoning_tool_and_done() {
    let chat = parse_stream_delta(
        ApiMode::ChatCompletions,
        &json!({
            "choices": [{
                "delta": {"content": "hi", "reasoning_content": "r"},
                "finish_reason": null
            }]
        }),
    );
    assert_eq!(chat.content_delta.as_deref(), Some("hi"));
    assert_eq!(chat.reasoning_delta.as_deref(), Some("r"));

    let responses = parse_stream_delta(
        ApiMode::Responses,
        &json!({
            "type": "response.output_text.delta",
            "delta": " chunk"
        }),
    );
    assert_eq!(responses.content_delta.as_deref(), Some(" chunk"));

    let anthropic = parse_stream_delta(ApiMode::Anthropic, &json!({"type": "message_stop"}));
    assert!(anthropic.done);
}

#[test]
fn provider_error_classification_matches_retry_buckets() {
    assert_eq!(
        classify_provider_error(Some(401), "nope"),
        ProviderErrorClass::Auth
    );
    assert_eq!(
        classify_provider_error(Some(429), "too many"),
        ProviderErrorClass::RateLimit
    );
    assert_eq!(
        classify_provider_error(None, "context_length_exceeded"),
        ProviderErrorClass::ContextOverflow
    );
    assert_eq!(
        classify_provider_error(Some(503), "busy"),
        ProviderErrorClass::Transient
    );
    assert_eq!(
        classify_provider_error(Some(400), "bad request"),
        ProviderErrorClass::Fatal
    );
}

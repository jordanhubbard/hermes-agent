//! Provider-specific request and response wire helpers.
//!
//! These helpers cover the provider-format edge of the agent core:
//! messages and tool definitions go in, provider-specific JSON goes
//! out; provider JSON responses come back in and normalize to an
//! [`AssistantTurn`]. They do not perform HTTP or credential work.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::budget::TokenUsage;
use crate::message::{AssistantTurn, Message};
use crate::provider::{ApiMode, ProviderRouting};
use crate::tool::{ToolCall, ToolDefinition};

/// Options that affect one provider request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderRequestOptions {
    /// Whether to ask the provider for streaming output.
    #[serde(default)]
    pub stream: bool,
    /// OpenAI service tier or equivalent provider-side priority hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Fallback model selected by the routing layer for this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_model: Option<String>,
}

/// Normalized provider response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedProviderResponse {
    /// Assistant turn normalized to Hermes' internal message shape.
    pub assistant: AssistantTurn,
    /// Usage reported by the provider, when present.
    #[serde(default)]
    pub usage: TokenUsage,
    /// Provider finish reason / stop reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Normalized streaming delta from a provider event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StreamDelta {
    /// Text delta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_delta: Option<String>,
    /// Reasoning delta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_delta: Option<String>,
    /// Completed tool call included in the stream event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ToolCall>,
    /// Whether this event terminates the stream.
    #[serde(default)]
    pub done: bool,
}

/// Provider error buckets used by retry/fallback policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorClass {
    /// Authentication or authorization failure.
    Auth,
    /// Rate limit / quota exhaustion.
    RateLimit,
    /// Context window overflow.
    ContextOverflow,
    /// Temporary service/network failure.
    Transient,
    /// Non-retriable provider error.
    Fatal,
}

/// Build the provider-specific request body for one turn.
pub fn build_provider_request(
    messages: &[Message],
    tools: &[ToolDefinition],
    routing: &ProviderRouting,
    options: &ProviderRequestOptions,
) -> Value {
    match routing.api_mode {
        ApiMode::ChatCompletions | ApiMode::OpenAiCompat => {
            let mut body = Map::new();
            body.insert("model".to_string(), json!(selected_model(routing, options)));
            body.insert("messages".to_string(), json!(messages));
            body.insert("stream".to_string(), json!(options.stream));
            if !tools.is_empty() {
                body.insert("tools".to_string(), json!(tools));
            }
            if let Some(service_tier) = &options.service_tier {
                body.insert("service_tier".to_string(), json!(service_tier));
            }
            Value::Object(body)
        }
        ApiMode::Responses => {
            let mut body = Map::new();
            body.insert("model".to_string(), json!(selected_model(routing, options)));
            body.insert("input".to_string(), json!(messages));
            body.insert("stream".to_string(), json!(options.stream));
            if !tools.is_empty() {
                body.insert("tools".to_string(), json!(tools));
            }
            if let Some(service_tier) = &options.service_tier {
                body.insert("service_tier".to_string(), json!(service_tier));
            }
            Value::Object(body)
        }
        ApiMode::Anthropic => {
            let mut body = Map::new();
            body.insert("model".to_string(), json!(selected_model(routing, options)));
            body.insert("messages".to_string(), json!(non_system_messages(messages)));
            body.insert("stream".to_string(), json!(options.stream));
            let system = system_prompt(messages);
            if !system.is_empty() {
                body.insert("system".to_string(), json!(system));
            }
            if !tools.is_empty() {
                body.insert("tools".to_string(), json!(anthropic_tools(tools)));
            }
            Value::Object(body)
        }
        ApiMode::Bedrock => {
            json!({
                "model": selected_model(routing, options),
                "messages": messages,
                "tools": tools,
                "stream": options.stream,
                "provider_options": routing.provider_options,
            })
        }
    }
}

fn selected_model<'a>(
    routing: &'a ProviderRouting,
    options: &'a ProviderRequestOptions,
) -> &'a str {
    options
        .fallback_model
        .as_deref()
        .unwrap_or(routing.model.as_str())
}

fn system_prompt(messages: &[Message]) -> String {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::System { content } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn non_system_messages(messages: &[Message]) -> Vec<Message> {
    messages
        .iter()
        .filter(|message| !matches!(message, Message::System { .. }))
        .cloned()
        .collect()
}

fn anthropic_tools(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.function.name,
                "description": tool.function.description,
                "input_schema": tool.function.parameters,
            })
        })
        .collect()
}

/// Parse a non-streaming provider response into an assistant turn.
pub fn parse_provider_response(
    api_mode: ApiMode,
    response: &Value,
) -> Result<ParsedProviderResponse, String> {
    match api_mode {
        ApiMode::ChatCompletions | ApiMode::OpenAiCompat => {
            parse_chat_completions_response(response)
        }
        ApiMode::Responses => parse_responses_response(response),
        ApiMode::Anthropic => parse_anthropic_response(response),
        ApiMode::Bedrock => parse_chat_completions_response(response),
    }
}

fn parse_chat_completions_response(response: &Value) -> Result<ParsedProviderResponse, String> {
    let choice = response
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| "chat response missing choices[0]".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "chat response missing choices[0].message".to_string())?;
    let assistant = AssistantTurn {
        content: message
            .get("content")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        tool_calls: parse_openai_tool_calls(message.get("tool_calls")),
        reasoning: message
            .get("reasoning")
            .or_else(|| message.get("reasoning_content"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
    };
    Ok(ParsedProviderResponse {
        assistant,
        usage: parse_usage(response.get("usage")),
        finish_reason: choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn parse_responses_response(response: &Value) -> Result<ParsedProviderResponse, String> {
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls = Vec::new();

    for item in response
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| "responses payload missing output".to_string())?
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for part in parts {
                        if matches!(
                            part.get("type").and_then(Value::as_str),
                            Some("output_text" | "text")
                        ) {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                content.push_str(text);
                            }
                        }
                    }
                }
            }
            Some("reasoning") => {
                if let Some(text) = item
                    .get("summary")
                    .and_then(Value::as_array)
                    .and_then(|summary| summary.first())
                    .and_then(|entry| entry.get("text"))
                    .and_then(Value::as_str)
                {
                    reasoning.push_str(text);
                }
            }
            Some("function_call") => {
                if let Some(call) = parse_response_function_call(item) {
                    tool_calls.push(call);
                }
            }
            _ => {}
        }
    }

    Ok(ParsedProviderResponse {
        assistant: AssistantTurn {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
            reasoning: if reasoning.is_empty() {
                None
            } else {
                Some(reasoning)
            },
        },
        usage: parse_usage(response.get("usage")),
        finish_reason: response
            .get("status")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn parse_anthropic_response(response: &Value) -> Result<ParsedProviderResponse, String> {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    for block in response
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "anthropic response missing content".to_string())?
    {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    content.push_str(text);
                }
            }
            Some("tool_use") => {
                if let Some(id) = block.get("id").and_then(Value::as_str) {
                    tool_calls.push(ToolCall {
                        id: id.to_string(),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        arguments: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(ParsedProviderResponse {
        assistant: AssistantTurn {
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
            reasoning: response
                .get("reasoning")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        },
        usage: parse_usage(response.get("usage")),
        finish_reason: response
            .get("stop_reason")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn parse_openai_tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    value
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let id = call.get("id")?.as_str()?.to_string();
                    let function = call.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let arguments = parse_arguments(function.get("arguments"));
                    Some(ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_response_function_call(item: &Value) -> Option<ToolCall> {
    Some(ToolCall {
        id: item
            .get("call_id")
            .or_else(|| item.get("id"))?
            .as_str()?
            .to_string(),
        name: item.get("name")?.as_str()?.to_string(),
        arguments: parse_arguments(item.get("arguments")),
    })
}

fn parse_arguments(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => serde_json::from_str(text).unwrap_or_else(|_| json!({})),
        Some(Value::Object(_)) => value.cloned().unwrap_or_else(|| json!({})),
        _ => json!({}),
    }
}

fn parse_usage(value: Option<&Value>) -> TokenUsage {
    let Some(value) = value else {
        return TokenUsage::default();
    };
    TokenUsage {
        input_tokens: pick_u64(value, &["input_tokens", "prompt_tokens"]),
        output_tokens: pick_u64(value, &["output_tokens", "completion_tokens"]),
        cache_read_tokens: pick_u64(value, &["cache_read_tokens"]),
        cache_write_tokens: pick_u64(value, &["cache_write_tokens"]),
        reasoning_tokens: value
            .pointer("/completion_tokens_details/reasoning_tokens")
            .and_then(Value::as_u64)
            .or_else(|| value.get("reasoning_tokens").and_then(Value::as_u64))
            .unwrap_or(0),
    }
}

fn pick_u64(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

/// Parse one streaming event into a normalized delta.
pub fn parse_stream_delta(api_mode: ApiMode, event: &Value) -> StreamDelta {
    match api_mode {
        ApiMode::ChatCompletions | ApiMode::OpenAiCompat | ApiMode::Bedrock => {
            let delta = event
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("delta"))
                .unwrap_or(event);
            StreamDelta {
                content_delta: delta
                    .get("content")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                reasoning_delta: delta
                    .get("reasoning")
                    .or_else(|| delta.get("reasoning_content"))
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                tool_call: parse_openai_tool_calls(delta.get("tool_calls"))
                    .into_iter()
                    .next(),
                done: event
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(|choice| choice.get("finish_reason"))
                    .is_some(),
            }
        }
        ApiMode::Responses => StreamDelta {
            content_delta: event
                .get("delta")
                .or_else(|| event.get("text"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
            reasoning_delta: event
                .get("reasoning_delta")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            tool_call: parse_response_function_call(event),
            done: matches!(
                event.get("type").and_then(Value::as_str),
                Some("response.completed")
            ),
        },
        ApiMode::Anthropic => StreamDelta {
            content_delta: event
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .map(str::to_string),
            reasoning_delta: event
                .pointer("/delta/thinking")
                .and_then(Value::as_str)
                .map(str::to_string),
            tool_call: None,
            done: matches!(
                event.get("type").and_then(Value::as_str),
                Some("message_stop")
            ),
        },
    }
}

/// Classify provider errors for retry/fallback policy.
pub fn classify_provider_error(status: Option<u16>, message: &str) -> ProviderErrorClass {
    let lower = message.to_ascii_lowercase();
    match status {
        Some(401 | 403) => ProviderErrorClass::Auth,
        Some(408 | 409 | 500 | 502 | 503 | 504) => ProviderErrorClass::Transient,
        Some(429) => ProviderErrorClass::RateLimit,
        _ if lower.contains("context") && lower.contains("exceed") => {
            ProviderErrorClass::ContextOverflow
        }
        _ if lower.contains("context_length") || lower.contains("maximum context") => {
            ProviderErrorClass::ContextOverflow
        }
        _ if lower.contains("rate limit") || lower.contains("quota") => {
            ProviderErrorClass::RateLimit
        }
        _ if lower.contains("unauthorized") || lower.contains("invalid api key") => {
            ProviderErrorClass::Auth
        }
        _ => ProviderErrorClass::Fatal,
    }
}

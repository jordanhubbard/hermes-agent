//! Blocking HTTP execution for provider requests.
//!
//! This is the first production-shaped provider boundary for the Rust agent
//! runtime. It intentionally reuses `provider_wire` for request and response
//! shapes so HTTP execution cannot drift from the existing parity fixtures.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::message::Message;
use crate::provider::{ApiMode, ProviderRouting};
use crate::provider_wire::{
    build_provider_request, classify_provider_error, parse_provider_response,
    ParsedProviderResponse, ProviderErrorClass, ProviderRequestOptions,
};
use crate::tool::ToolDefinition;

/// HTTP options for one provider request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpOptions {
    /// Optional bearer token. Extra headers may override Authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl Default for ProviderHttpOptions {
    fn default() -> Self {
        Self {
            api_key: None,
            timeout_secs: default_timeout_secs(),
        }
    }
}

fn default_timeout_secs() -> u64 {
    60
}

/// Successful provider HTTP response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderHttpResponse {
    /// Final URL called.
    pub url: String,
    /// HTTP status code.
    pub status: u16,
    /// Raw JSON body returned by the provider.
    pub raw_json: Value,
    /// Parsed assistant turn and usage.
    pub parsed: ParsedProviderResponse,
}

/// Provider HTTP failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpError {
    /// Final URL attempted when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// HTTP status code for provider responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Retry/fallback classification.
    pub class: ProviderErrorClass,
    /// Human-readable message.
    pub message: String,
}

/// Execute one non-streaming provider request over HTTP.
pub fn execute_provider_request(
    messages: &[Message],
    tools: &[ToolDefinition],
    routing: &ProviderRouting,
    request_options: &ProviderRequestOptions,
    http_options: &ProviderHttpOptions,
) -> Result<ProviderHttpResponse, ProviderHttpError> {
    if request_options.stream {
        return Err(ProviderHttpError {
            url: None,
            status: None,
            class: ProviderErrorClass::Fatal,
            message: "streaming HTTP execution is not implemented by execute_provider_request"
                .to_string(),
        });
    }

    let url = provider_url(routing).map_err(|message| ProviderHttpError {
        url: None,
        status: None,
        class: ProviderErrorClass::Fatal,
        message,
    })?;
    let body = build_provider_request(messages, tools, routing, request_options);
    let body_text = serde_json::to_string(&body).map_err(|err| ProviderHttpError {
        url: Some(url.clone()),
        status: None,
        class: ProviderErrorClass::Fatal,
        message: format!("provider request did not serialize: {err}"),
    })?;

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(http_options.timeout_secs))
        .build();
    let mut request = agent.post(&url).set("content-type", "application/json");
    if let Some(api_key) = http_options
        .api_key
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        request = request.set("authorization", &format!("Bearer {api_key}"));
    }
    for (name, value) in &routing.extra_headers {
        request = request.set(name, value);
    }

    let response = match request.send_string(&body_text) {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => {
            let message = response
                .into_string()
                .unwrap_or_else(|err| format!("failed to read provider error body: {err}"));
            return Err(ProviderHttpError {
                url: Some(url),
                status: Some(status),
                class: classify_provider_error(Some(status), &message),
                message,
            });
        }
        Err(ureq::Error::Transport(err)) => {
            return Err(ProviderHttpError {
                url: Some(url),
                status: None,
                class: ProviderErrorClass::Transient,
                message: err.to_string(),
            });
        }
    };

    let status = response.status();
    let raw_text = response.into_string().map_err(|err| ProviderHttpError {
        url: Some(url.clone()),
        status: Some(status),
        class: ProviderErrorClass::Transient,
        message: format!("failed to read provider response body: {err}"),
    })?;
    let raw_json: Value = serde_json::from_str(&raw_text).map_err(|err| ProviderHttpError {
        url: Some(url.clone()),
        status: Some(status),
        class: ProviderErrorClass::Fatal,
        message: format!("provider response was not JSON: {err}"),
    })?;
    let parsed = parse_provider_response(routing.api_mode, &raw_json).map_err(|message| {
        ProviderHttpError {
            url: Some(url.clone()),
            status: Some(status),
            class: ProviderErrorClass::Fatal,
            message,
        }
    })?;

    Ok(ProviderHttpResponse {
        url,
        status,
        raw_json,
        parsed,
    })
}

fn provider_url(routing: &ProviderRouting) -> Result<String, String> {
    let base = routing
        .base_url
        .as_deref()
        .unwrap_or(default_base_url(routing.api_mode))
        .trim_end_matches('/');
    let path = match routing.api_mode {
        ApiMode::ChatCompletions | ApiMode::OpenAiCompat | ApiMode::Bedrock => "/chat/completions",
        ApiMode::Responses => "/responses",
        ApiMode::Anthropic => "/messages",
    };
    if base.is_empty() {
        return Err("provider base_url is empty".to_string());
    }
    Ok(format!("{base}{path}"))
}

fn default_base_url(api_mode: ApiMode) -> &'static str {
    match api_mode {
        ApiMode::ChatCompletions | ApiMode::OpenAiCompat | ApiMode::Responses => {
            "https://api.openai.com/v1"
        }
        ApiMode::Anthropic => "https://api.anthropic.com/v1",
        ApiMode::Bedrock => "",
    }
}

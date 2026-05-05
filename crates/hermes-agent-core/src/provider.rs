//! Provider routing inputs.
//!
//! The Python side resolves these from a mix of config, environment,
//! credential sources, and runtime overrides. The agent loop only
//! needs the resolved shape — what API to call, what model to pass,
//! what base URL and headers, and what conversation-format to use.
//!
//! These types are the contract between the resolution layer (Python
//! today) and the request-builder (Rust eventually). They are
//! intentionally provider-agnostic.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// API protocol the agent uses to talk to the provider on this turn.
///
/// Naming matches the Python `api_mode` value tracked by
/// `hermes_cli/runtime_provider.py`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiMode {
    /// OpenAI Chat Completions (`POST /v1/chat/completions`).
    ChatCompletions,
    /// OpenAI Responses (`POST /v1/responses`).
    Responses,
    /// Native Anthropic Messages (`POST /v1/messages`).
    Anthropic,
    /// AWS Bedrock InvokeModel.
    Bedrock,
    /// OpenAI-compatible endpoint that does not advertise itself as
    /// either ChatCompletions or Responses.
    OpenAiCompat,
}

/// Resolved provider routing for one agent turn.
///
/// Designed so that an entire turn's request can be reconstructed from
/// `(messages, tool_definitions, ProviderRouting, ConversationBudget)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRouting {
    /// Provider identifier (e.g. `"openai"`, `"anthropic"`,
    /// `"openrouter"`, `"bedrock"`, `"custom"`). Free-form to mirror
    /// Python's open set.
    pub provider: String,
    /// Model identifier as the provider expects to receive it.
    pub model: String,
    /// Resolved base URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// API protocol selected for this turn.
    pub api_mode: ApiMode,
    /// Extra HTTP headers (auth tokens, beta headers, vendor
    /// passthroughs). Sorted-key map so the wire shape is stable.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
    /// Provider-specific options the request builder must pass through
    /// (e.g. Bedrock region, Anthropic `anthropic-version`, Responses
    /// `service_tier`). Free-form JSON to avoid coupling this crate to
    /// every provider's quirk surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
}

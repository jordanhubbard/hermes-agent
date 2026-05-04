//! Tool calls, results, and definitions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One tool invocation issued by the assistant on a turn.
///
/// Shape matches the parity fixtures: `arguments` is a JSON object,
/// not a JSON-encoded string (which is the OpenAI wire format the
/// Python side parses on entry). The fixtures normalize the parsed
/// shape so backends can compare structurally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Stable identifier for this call, used to bind a [`ToolResult`].
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Parsed argument object.
    pub arguments: Value,
}

/// Result of a single tool invocation.
///
/// This is the structured form the dispatcher produces. It is converted
/// into a [`crate::message::ToolTurn`] before being persisted as a
/// conversation message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// `tool_call.id` this result is for.
    pub call_id: String,
    /// Whether the tool succeeded.
    pub ok: bool,
    /// Result body — typically the tool's stdout / structured response.
    pub content: String,
}

/// Tool definition advertised to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Currently always `"function"` for OpenAI-compatible tools.
    #[serde(rename = "type")]
    pub kind: String,
    /// Function metadata + JSON Schema for the parameters.
    pub function: ToolFunction,
}

/// Function metadata for a [`ToolDefinition`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolFunction {
    /// Tool name.
    pub name: String,
    /// Free-form description shown to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the tool's arguments.
    pub parameters: Value,
}

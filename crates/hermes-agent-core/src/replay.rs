//! Deterministic replay engine for parity fixtures.
//!
//! The replay fixture format lives in `tests/parity/fixtures`. This
//! module intentionally does not call a provider or execute real tools:
//! it exercises the same assistant/tool message append contract using
//! canned model responses and canned tool results.

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation_loop::{
    run_canned_conversation, CannedConversationConfig, CannedConversationInput,
};
use crate::message::{AssistantTurn, Message};
use crate::tool::{ToolCall, ToolResult};

/// Error returned when a replay fixture is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    message: String,
}

impl ReplayError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReplayError {}

/// Captured behavior from one replay of a fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplayResult {
    /// Number of canned model responses consumed.
    pub turn_count: u32,
    /// Tool dispatches in encounter order.
    pub tool_calls_dispatched: Vec<DispatchedToolCall>,
    /// Tool results persisted in encounter order.
    pub tool_results_persisted: Vec<PersistedToolResult>,
    /// Final assistant message, if the replay ended on one.
    pub final_message: Option<FinalMessage>,
    /// All persisted messages.
    pub persisted_messages: Vec<Message>,
    /// Whether any assistant turn carried a reasoning field.
    pub reasoning_fields_present: bool,
    /// Replay-level errors.
    pub errors: Vec<String>,
}

/// Projection of one dispatched tool call used by parity checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchedToolCall {
    /// Tool name.
    pub name: String,
    /// Sorted argument keys.
    pub argument_keys: Vec<String>,
}

/// Projection of one persisted tool result used by parity checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedToolResult {
    /// Assistant tool-call ID this result answers.
    pub call_id: String,
    /// Whether the tool succeeded.
    pub ok: bool,
}

/// Final assistant message projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalMessage {
    /// Always `"assistant"`.
    pub role: String,
    /// Assistant text content.
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    inputs: FixtureInputs,
}

#[derive(Debug, Deserialize)]
struct FixtureInputs {
    user_messages: Vec<String>,
    #[serde(default)]
    canned_model_responses: Vec<CannedModelResponse>,
    #[serde(default)]
    canned_tool_results: HashMap<String, CannedToolResult>,
}

#[derive(Debug, Deserialize)]
struct CannedModelResponse {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
    #[serde(default)]
    reasoning: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CannedToolResult {
    #[serde(default = "default_ok")]
    ok: bool,
    #[serde(default)]
    content: Option<String>,
}

fn default_ok() -> bool {
    true
}

/// Replay one backend-agnostic parity fixture.
pub fn replay_fixture(fixture: Value) -> Result<ReplayResult, ReplayError> {
    let fixture: Fixture = serde_json::from_value(fixture)
        .map_err(|err| ReplayError::new(format!("invalid fixture: {err}")))?;
    if fixture.inputs.user_messages.is_empty() {
        return Err(ReplayError::new("fixture has no user messages"));
    }
    if fixture.inputs.canned_model_responses.is_empty() {
        return Err(ReplayError::new("fixture has no canned model responses"));
    }

    let mut tool_results = HashMap::new();
    for (call_id, canned) in fixture.inputs.canned_tool_results {
        tool_results.insert(
            call_id.clone(),
            ToolResult {
                call_id,
                ok: canned.ok,
                content: canned.content.unwrap_or_default(),
            },
        );
    }

    let loop_result = run_canned_conversation(
        CannedConversationInput {
            user_messages: fixture.inputs.user_messages,
            model_responses: fixture
                .inputs
                .canned_model_responses
                .into_iter()
                .map(|response| AssistantTurn {
                    content: response.content,
                    tool_calls: response.tool_calls,
                    reasoning: response.reasoning,
                })
                .collect(),
            tool_results,
        },
        CannedConversationConfig::default(),
    );

    let reasoning_fields_present = loop_result.messages.iter().any(|message| {
        matches!(
            message,
            Message::Assistant(turn)
                if turn.reasoning.as_ref().map(|s| !s.is_empty()).unwrap_or(false)
        )
    });
    let final_message = loop_result.final_response.map(|content| FinalMessage {
        role: "assistant".to_string(),
        content: Some(content),
    });

    Ok(ReplayResult {
        turn_count: loop_result.model_call_count,
        tool_calls_dispatched: loop_result
            .tool_calls_dispatched
            .iter()
            .map(|tool_call| DispatchedToolCall {
                name: tool_call.name.clone(),
                argument_keys: sorted_argument_keys(&tool_call.arguments),
            })
            .collect(),
        tool_results_persisted: loop_result
            .tool_results_persisted
            .iter()
            .map(|result| PersistedToolResult {
                call_id: result.call_id.clone(),
                ok: result.ok,
            })
            .collect(),
        final_message,
        persisted_messages: loop_result.messages,
        reasoning_fields_present,
        errors: loop_result.errors,
    })
}

fn sorted_argument_keys(arguments: &Value) -> Vec<String> {
    match arguments.as_object() {
        Some(obj) => obj
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::replay_fixture;

    #[test]
    fn replay_tool_call_fixture_shape() {
        let result = replay_fixture(json!({
            "inputs": {
                "user_messages": ["read it"],
                "canned_model_responses": [
                    {
                        "content": null,
                        "tool_calls": [
                            {"id": "c1", "name": "read_file", "arguments": {"path": "README.md"}}
                        ]
                    },
                    {"content": "done"}
                ],
                "canned_tool_results": {
                    "c1": {"ok": true, "content": "Hermes Agent"}
                }
            }
        }))
        .unwrap();

        assert_eq!(result.turn_count, 2);
        assert_eq!(result.persisted_messages.len(), 4);
        assert_eq!(result.tool_calls_dispatched[0].name, "read_file");
        assert_eq!(result.tool_results_persisted[0].call_id, "c1");
        assert_eq!(
            result.final_message.unwrap().content.as_deref(),
            Some("done")
        );
    }
}

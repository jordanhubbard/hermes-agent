//! Synchronous conversation/tool-call loop.
//!
//! This is the provider-independent core of `run_agent.py`: call the
//! model, append the assistant turn, dispatch tool calls in order,
//! append tool results, and stop when the model returns a final
//! assistant message or a loop guard fires. Provider HTTP request
//! construction and real tool handlers live behind separate migration
//! beads.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::message::{AssistantTurn, Message, ToolTurn};
use crate::tool::{ToolCall, ToolResult};

/// Configuration for one deterministic loop run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CannedConversationConfig {
    /// Max tool-call iterations before the loop stops.
    pub max_iterations: u32,
    /// Remaining iteration budget shared with parent/child agents.
    pub iteration_budget_remaining: u32,
    /// Whether one final model call is allowed after the regular
    /// limits are exhausted.
    #[serde(default)]
    pub budget_grace_call: bool,
    /// Interrupt before making the model call at this zero-based model
    /// call index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_before_call: Option<u32>,
}

impl Default for CannedConversationConfig {
    fn default() -> Self {
        Self {
            max_iterations: 90,
            iteration_budget_remaining: u32::MAX,
            budget_grace_call: false,
            interrupt_before_call: None,
        }
    }
}

/// Inputs for one deterministic loop run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CannedConversationInput {
    /// User messages to seed into the persisted transcript.
    pub user_messages: Vec<String>,
    /// Canned model responses in call order.
    pub model_responses: Vec<AssistantTurn>,
    /// Tool results keyed by `tool_call.id`.
    #[serde(default)]
    pub tool_results: HashMap<String, ToolResult>,
}

/// Reason the loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The model returned a final assistant message with no tool calls.
    Completed,
    /// The loop reached max_iterations or exhausted the shared
    /// iteration budget.
    IterationLimit,
    /// The interrupt flag was set before the next model call.
    Interrupted,
    /// No canned model response existed for the next call.
    ModelResponsesExhausted,
}

/// Captured result from one loop run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CannedConversationResult {
    /// Why the loop stopped.
    pub stop_reason: StopReason,
    /// Final response content when stop_reason is `Completed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,
    /// Number of model calls made.
    pub model_call_count: u32,
    /// Number of tool-call iterations completed.
    pub tool_iteration_count: u32,
    /// Full persisted transcript.
    pub messages: Vec<Message>,
    /// Tool calls dispatched in exact encounter order.
    pub tool_calls_dispatched: Vec<ToolCall>,
    /// Tool results persisted in exact encounter order.
    pub tool_results_persisted: Vec<ToolResult>,
    /// Loop-level errors, such as missing canned tool results.
    pub errors: Vec<String>,
}

/// Run the synchronous agent loop over deterministic model/tool inputs.
pub fn run_canned_conversation(
    input: CannedConversationInput,
    config: CannedConversationConfig,
) -> CannedConversationResult {
    let mut messages = input
        .user_messages
        .into_iter()
        .map(Message::user)
        .collect::<Vec<_>>();
    let mut model_responses = input.model_responses.into_iter();
    let mut grace_call = config.budget_grace_call;
    let mut model_call_count = 0_u32;
    let mut tool_iteration_count = 0_u32;
    let mut tool_calls_dispatched = Vec::new();
    let mut tool_results_persisted = Vec::new();
    let mut errors = Vec::new();

    loop {
        let regular_budget_allows = tool_iteration_count < config.max_iterations
            && model_call_count < config.iteration_budget_remaining;
        if !regular_budget_allows && !grace_call {
            return CannedConversationResult {
                stop_reason: StopReason::IterationLimit,
                final_response: None,
                model_call_count,
                tool_iteration_count,
                messages,
                tool_calls_dispatched,
                tool_results_persisted,
                errors,
            };
        }

        if config.interrupt_before_call == Some(model_call_count) {
            return CannedConversationResult {
                stop_reason: StopReason::Interrupted,
                final_response: None,
                model_call_count,
                tool_iteration_count,
                messages,
                tool_calls_dispatched,
                tool_results_persisted,
                errors,
            };
        }

        if !regular_budget_allows {
            grace_call = false;
        }

        let assistant = match model_responses.next() {
            Some(turn) => turn,
            None => {
                return CannedConversationResult {
                    stop_reason: StopReason::ModelResponsesExhausted,
                    final_response: None,
                    model_call_count,
                    tool_iteration_count,
                    messages,
                    tool_calls_dispatched,
                    tool_results_persisted,
                    errors,
                }
            }
        };
        model_call_count = model_call_count.saturating_add(1);

        let tool_calls = assistant.tool_calls.clone();
        let final_response = assistant.content.clone();
        messages.push(Message::Assistant(assistant));

        if tool_calls.is_empty() {
            return CannedConversationResult {
                stop_reason: StopReason::Completed,
                final_response,
                model_call_count,
                tool_iteration_count,
                messages,
                tool_calls_dispatched,
                tool_results_persisted,
                errors,
            };
        }

        for tool_call in tool_calls {
            tool_calls_dispatched.push(tool_call.clone());
            match input.tool_results.get(&tool_call.id) {
                Some(result) => {
                    messages.push(Message::Tool(ToolTurn {
                        tool_call_id: tool_call.id,
                        name: Some(tool_call.name),
                        content: result.content.clone(),
                        ok: Some(result.ok),
                    }));
                    tool_results_persisted.push(result.clone());
                }
                None => errors.push(format!(
                    "missing canned result for tool_call {}",
                    tool_call.id
                )),
            }
        }
        tool_iteration_count = tool_iteration_count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::{
        run_canned_conversation, AssistantTurn, CannedConversationConfig, CannedConversationInput,
        Message, StopReason, ToolCall, ToolResult,
    };

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: json!({"path": "README.md"}),
        }
    }

    #[test]
    fn appends_assistant_tool_and_final_messages_in_order() {
        let tool_call = call("c1", "read_file");
        let mut tool_results = HashMap::new();
        tool_results.insert(
            "c1".to_string(),
            ToolResult {
                call_id: "c1".to_string(),
                ok: true,
                content: "Hermes Agent".to_string(),
            },
        );

        let result = run_canned_conversation(
            CannedConversationInput {
                user_messages: vec!["read".to_string()],
                model_responses: vec![
                    AssistantTurn {
                        content: None,
                        tool_calls: vec![tool_call],
                        reasoning: None,
                    },
                    AssistantTurn {
                        content: Some("done".to_string()),
                        ..AssistantTurn::default()
                    },
                ],
                tool_results,
            },
            CannedConversationConfig::default(),
        );

        assert_eq!(result.stop_reason, StopReason::Completed);
        assert_eq!(result.final_response.as_deref(), Some("done"));
        assert_eq!(result.model_call_count, 2);
        assert_eq!(result.tool_iteration_count, 1);
        assert_eq!(
            result
                .messages
                .iter()
                .map(Message::role)
                .collect::<Vec<_>>()
                .len(),
            4
        );
        assert_eq!(result.tool_calls_dispatched[0].id, "c1");
        assert_eq!(result.tool_results_persisted[0].call_id, "c1");
    }

    #[test]
    fn stops_at_iteration_limit_without_grace_call() {
        let result = run_canned_conversation(
            CannedConversationInput {
                user_messages: vec!["x".to_string()],
                model_responses: vec![AssistantTurn {
                    content: Some("should not be called".to_string()),
                    ..AssistantTurn::default()
                }],
                tool_results: HashMap::new(),
            },
            CannedConversationConfig {
                max_iterations: 0,
                iteration_budget_remaining: 1,
                budget_grace_call: false,
                interrupt_before_call: None,
            },
        );
        assert_eq!(result.stop_reason, StopReason::IterationLimit);
        assert_eq!(result.model_call_count, 0);
    }

    #[test]
    fn grace_call_allows_one_final_response() {
        let result = run_canned_conversation(
            CannedConversationInput {
                user_messages: vec!["x".to_string()],
                model_responses: vec![AssistantTurn {
                    content: Some("grace final".to_string()),
                    ..AssistantTurn::default()
                }],
                tool_results: HashMap::new(),
            },
            CannedConversationConfig {
                max_iterations: 0,
                iteration_budget_remaining: 0,
                budget_grace_call: true,
                interrupt_before_call: None,
            },
        );
        assert_eq!(result.stop_reason, StopReason::Completed);
        assert_eq!(result.final_response.as_deref(), Some("grace final"));
        assert_eq!(result.model_call_count, 1);
    }

    #[test]
    fn interrupt_is_checked_before_model_call() {
        let result = run_canned_conversation(
            CannedConversationInput {
                user_messages: vec!["x".to_string()],
                model_responses: vec![AssistantTurn {
                    content: Some("should not be called".to_string()),
                    ..AssistantTurn::default()
                }],
                tool_results: HashMap::new(),
            },
            CannedConversationConfig {
                interrupt_before_call: Some(0),
                ..CannedConversationConfig::default()
            },
        );
        assert_eq!(result.stop_reason, StopReason::Interrupted);
        assert_eq!(result.model_call_count, 0);
    }
}

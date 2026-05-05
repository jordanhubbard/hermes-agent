//! Interrupt and conversation-outcome types.
//!
//! `run_agent.py` ends every conversation with one of a small number
//! of well-known outcomes (final answer, user interrupt, max-turn cap,
//! provider error, context overflow). The agent loop signals an
//! `Interrupt` to abort early; the loop then resolves to a
//! `ConversationOutcome` value that the transport returns.

use serde::{Deserialize, Serialize};

use crate::budget::ConversationBudget;

/// Reason an in-flight turn was interrupted. The agent loop converts
/// these into a `ConversationOutcome` when unwinding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptKind {
    /// User pressed `^C` or sent the platform's interrupt signal.
    User,
    /// Slash command issued an explicit stop (e.g. `/stop`).
    SlashStop,
    /// `max_turns` budget exhausted before the model converged.
    MaxTurns,
    /// Tool call exceeded its allowed runtime.
    ToolTimeout,
    /// Tool refused (e.g. approval denied).
    ApprovalDenied,
    /// External signal (gateway shutdown, batch cancel).
    External,
}

/// Why the conversation stopped. One of these is always the result of
/// `run_conversation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConversationOutcome {
    /// The model returned a final assistant message.
    Completed {
        /// Final assistant message content.
        final_message: String,
    },
    /// Loop unwound early because of an [`InterruptKind`].
    Interrupted {
        /// Why the loop unwound.
        reason: InterruptKind,
        /// Optional human-readable context.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Provider returned an error that the agent could not recover
    /// from on this turn.
    ProviderError {
        /// Provider-supplied error string.
        error: String,
    },
    /// Context window overflowed and compression failed (or was
    /// disabled). Distinct from `Interrupted` so callers can offer a
    /// "compress and retry" path.
    ContextOverflow,
    /// A tool call repeatedly failed. The agent loop's guardrail
    /// converts this into a final stop.
    ToolLoop {
        /// Name of the tool that looped.
        tool_name: String,
    },
}

/// Wrapper holding outcome plus the budget snapshot at termination.
/// This is what the transport returns to the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationResult {
    /// Outcome variant.
    pub outcome: ConversationOutcome,
    /// Budget snapshot at the moment the loop stopped.
    pub budget: ConversationBudget,
}

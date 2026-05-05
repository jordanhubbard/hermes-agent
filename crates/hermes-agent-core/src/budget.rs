//! Token usage, cost, and per-turn budget state.
//!
//! These types model what `run_agent.py` tracks across a turn. The
//! field names match `hermes_state.TokenUpdate` (which is the database
//! contract) so values can be passed through without reshape.

use serde::{Deserialize, Serialize};

/// Token counts for one turn or one session.
///
/// All fields are saturating counts — they never go negative. Wire
/// shape matches `hermes_state.TokenUpdate` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// Prompt / input tokens consumed.
    #[serde(default)]
    pub input_tokens: u64,
    /// Output / completion tokens produced.
    #[serde(default)]
    pub output_tokens: u64,
    /// Tokens served from the prompt cache.
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Tokens written into the prompt cache on this turn.
    #[serde(default)]
    pub cache_write_tokens: u64,
    /// Reasoning tokens (Anthropic / Codex / DeepSeek-style).
    #[serde(default)]
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    /// Add another usage block into self (e.g. accumulating across turns).
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self.cache_read_tokens.saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(other.reasoning_tokens);
    }

    /// Sum of all token counts billed against the context window.
    ///
    /// The Python side treats input + cache_read as "in-context",
    /// output + reasoning as "produced", and cache_write as a write-side
    /// metric that's not directly comparable to a window budget. We
    /// only sum the tokens that occupy the window here.
    pub fn total_in_context(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.output_tokens)
            .saturating_add(self.reasoning_tokens)
    }
}

/// Per-turn cost in USD. Floating-point — costs are inherently fuzzy.
///
/// Matches the cost columns on `hermes_state.SessionDB`: a separate
/// estimated/actual split because some providers bill async and the
/// estimated value is the agent's best guess at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TurnCost {
    /// Estimated cost as computed at the end of the turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
    /// Authoritative cost when the provider reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_cost_usd: Option<f64>,
}

/// Snapshot of where a conversation stands relative to its budgets.
///
/// `model_context_limit` is provider/model metadata; the loop compares
/// `usage.total_in_context()` against it before deciding whether to
/// trigger compression.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ConversationBudget {
    /// Cumulative usage across the conversation so far.
    #[serde(default)]
    pub usage: TokenUsage,
    /// Max tokens the model accepts in one request, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_context_limit: Option<u64>,
    /// Hard cap on output tokens for the next turn, if the agent is
    /// asking the provider to truncate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// Number of agent turns elapsed.
    #[serde(default)]
    pub turn_count: u32,
    /// Hard cap on agent turns for this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
}

impl ConversationBudget {
    /// `true` when the agent loop should stop adding turns.
    pub fn turns_exhausted(&self) -> bool {
        match self.max_turns {
            Some(limit) => self.turn_count >= limit,
            None => false,
        }
    }

    /// `true` when the cumulative usage is at or above the model's
    /// declared context window. Caller decides whether to compress
    /// or fail.
    pub fn context_exhausted(&self) -> bool {
        match self.model_context_limit {
            Some(limit) => self.usage.total_in_context() >= limit,
            None => false,
        }
    }
}

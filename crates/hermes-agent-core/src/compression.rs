//! Compression metadata.
//!
//! When the conversation history exceeds the model's context window,
//! `agent/context_compressor.py` summarizes the middle turns and splits
//! the session lineage so the survived head/tail messages live on a
//! new session that points back at the parent. These types model that
//! metadata for cross-language consumers.

use serde::{Deserialize, Serialize};

use crate::budget::TokenUsage;

/// Why a compression event happened. Mirrors the strings the Python
/// side persists into `sessions.end_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionTrigger {
    /// Cumulative usage crossed the model's context window.
    ContextLimit,
    /// User explicitly asked to compress (`/compress`).
    UserRequested,
    /// Auxiliary heuristic decided context pressure was rising.
    HeuristicPressure,
    /// Provider reported a context-overflow error and the agent
    /// recovered by compressing.
    ProviderOverflow,
}

/// One compression event recorded against a session.
///
/// The Python side persists this as a chain of session rows linked by
/// `parent_session_id`; on the Rust side we keep the chain explicit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionEvent {
    /// Session ID that existed before compression.
    pub parent_session_id: String,
    /// Session ID created to hold post-compression head/tail messages.
    pub child_session_id: String,
    /// What triggered this compression.
    pub trigger: CompressionTrigger,
    /// Number of middle messages folded into the summary.
    pub dropped_message_count: u32,
    /// Token usage observed at the moment compression fired.
    pub usage_at_trigger: TokenUsage,
    /// The summary text that replaces the dropped middle messages.
    pub summary: String,
    /// Provider error string if `trigger` is `ProviderOverflow`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<String>,
}

/// Lineage tip pointer — what a Python `session_search` row carries to
/// say "this session is the live continuation of the chain rooted at
/// `root_session_id`."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineageTip {
    /// Live tip session ID (the most recent compression child).
    pub tip_session_id: String,
    /// Earliest ancestor in the chain.
    pub root_session_id: String,
    /// Number of compression events in the chain (length of the chain
    /// minus one).
    #[serde(default)]
    pub depth: u32,
}

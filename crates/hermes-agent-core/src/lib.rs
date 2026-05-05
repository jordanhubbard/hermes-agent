//! Hermes agent-core domain model.
//!
//! Standalone Rust types for the conversation surface that the Python
//! `AIAgent` exposes. The shapes match the parity fixtures in
//! `tests/parity/fixtures/`, which are the cross-backend contract.
//!
//! Tracked by bead `hermes-1oa.1` (define Rust agent-core domain model).
//!
//! Crate-level rule (see `docs/rust-parity/README.md` constraints):
//! this crate is a standalone reimplementation. It must not link to
//! in-repo Python code, must not be built as a Python extension, and
//! must not embed CPython.
//!
//! Scope:
//!   * Messages (system/user/assistant/tool).
//!   * Tool calls, results, definitions.
//!   * Reasoning field on assistant turns.
//!   * Budget / token accounting state.
//!   * Compression metadata and lineage.
//!   * Provider routing inputs.
//!   * Interrupt and conversation outcome types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod budget;
pub mod compression;
pub mod compression_plan;
pub mod conversation_loop;
pub mod message;
pub mod outcome;
pub mod provider;
pub mod provider_http;
pub mod provider_wire;
pub mod replay;
pub mod tool;

pub use budget::{ConversationBudget, TokenUsage, TurnCost};
pub use compression::{CompressionEvent, CompressionTrigger, LineageTip};
pub use compression_plan::{plan_compression, CompressionPlan, CompressionPlanOptions};
pub use conversation_loop::{
    run_canned_conversation, CannedConversationConfig, CannedConversationInput,
    CannedConversationResult, StopReason,
};
pub use message::{AssistantTurn, Message, Role, ToolTurn};
pub use outcome::{ConversationOutcome, ConversationResult, InterruptKind};
pub use provider::{ApiMode, ProviderRouting};
pub use provider_http::{
    execute_provider_request, ProviderHttpError, ProviderHttpOptions, ProviderHttpResponse,
};
pub use provider_wire::{
    build_provider_request, classify_provider_error, parse_provider_response, parse_stream_delta,
    ParsedProviderResponse, ProviderErrorClass, ProviderRequestOptions, StreamDelta,
};
pub use replay::{replay_fixture, ReplayError, ReplayResult};
pub use tool::{ToolCall, ToolDefinition, ToolFunction, ToolResult};

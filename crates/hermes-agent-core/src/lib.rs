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
//! Scope of this initial scaffolding:
//!   * Messages (system/user/assistant/tool).
//!   * Tool calls and tool results.
//!   * Tool definitions (OpenAI-style function tools).
//!   * Reasoning field on assistant turns.
//!
//! Out of scope here, intentionally — these arrive in subsequent ticks
//! against the same bead:
//!   * Budget / token accounting state.
//!   * Compression metadata and lineage.
//!   * Provider routing inputs.
//!   * Interrupt and conversation outcome types.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod message;
pub mod tool;

pub use message::{AssistantTurn, Message, Role, ToolTurn};
pub use tool::{ToolCall, ToolDefinition, ToolFunction, ToolResult};

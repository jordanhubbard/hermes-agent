//! Rust state-store primitives for Hermes Agent.
//!
//! This crate is the first migration step for `hermes_state.py`. It keeps the
//! Python runtime authoritative for now, while porting deterministic pieces
//! that can be tested independently and reused by a future Rust-backed session
//! database implementation.

pub mod schema;
pub mod search;
pub mod store;
pub mod title;

pub use schema::{FTS_SQL, FTS_TRIGRAM_SQL, SCHEMA_SQL, SCHEMA_VERSION};
pub use search::{contains_cjk, count_cjk, is_cjk_codepoint, sanitize_fts5_query};
pub use store::{
    AppendMessage, ConversationMessage, ExportedSession, NewSession, SearchContextMessage,
    SearchMatch, SearchOptions, SessionListOptions, SessionListRecord, SessionRecord,
    SessionRichOptions, SessionRichRecord, SessionStore, StateError, StateResult, StoredMessage,
    TokenUpdate,
};
pub use title::{sanitize_title, TitleError, MAX_TITLE_LENGTH};

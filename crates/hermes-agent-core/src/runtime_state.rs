//! Rust state-backed persistence for the agent runtime.
//!
//! This adapter writes runtime messages, compression lineage, and token totals
//! through the shared Rust `hermes-state` crate so Rust runtime tests exercise
//! the same SQLite schema as Python `SessionDB` parity tests.

use hermes_state::{AppendMessage, NewSession, SessionStore, StateError, TokenUpdate};
use serde_json::Value;
use std::path::Path;

use crate::budget::ConversationBudget;
use crate::compression::CompressionEvent;
use crate::message::Message;
use crate::runtime::ConversationStore;

/// Session metadata used when opening a state-backed runtime store.
#[derive(Debug, Clone, PartialEq)]
pub struct StateStoreOptions {
    /// Session ID to create or resume.
    pub session_id: String,
    /// Session source, e.g. `cli`, `tui`, or a gateway platform.
    pub source: String,
    /// Optional user identifier.
    pub user_id: Option<String>,
    /// Optional model name.
    pub model: Option<String>,
    /// Optional model config JSON.
    pub model_config: Option<Value>,
    /// Optional system prompt snapshot.
    pub system_prompt: Option<String>,
    /// Optional parent session for lineage.
    pub parent_session_id: Option<String>,
}

impl StateStoreOptions {
    /// Create options for a session ID and source.
    pub fn new(session_id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            source: source.into(),
            user_id: None,
            model: None,
            model_config: None,
            system_prompt: None,
            parent_session_id: None,
        }
    }

    /// Set the user ID.
    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set the model name.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the model config JSON.
    pub fn model_config(mut self, model_config: Value) -> Self {
        self.model_config = Some(model_config);
        self
    }

    /// Set the system prompt.
    pub fn system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    /// Set the parent session ID.
    pub fn parent_session_id(mut self, parent_session_id: impl Into<String>) -> Self {
        self.parent_session_id = Some(parent_session_id.into());
        self
    }
}

/// A [`ConversationStore`] backed by [`SessionStore`].
pub struct StateConversationStore {
    store: SessionStore,
    options: StateStoreOptions,
    active_session_id: String,
    errors: Vec<String>,
}

impl std::fmt::Debug for StateConversationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateConversationStore")
            .field("options", &self.options)
            .field("active_session_id", &self.active_session_id)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

impl StateConversationStore {
    /// Open a SQLite-backed store and ensure the active session exists.
    pub fn open(path: impl AsRef<Path>, options: StateStoreOptions) -> Result<Self, StateError> {
        Self::from_store(SessionStore::open(path)?, options)
    }

    /// Wrap an existing [`SessionStore`] and ensure the active session exists.
    pub fn from_store(
        mut store: SessionStore,
        options: StateStoreOptions,
    ) -> Result<Self, StateError> {
        create_session_from_options(&mut store, &options)?;
        let active_session_id = options.session_id.clone();
        Ok(Self {
            store,
            options,
            active_session_id,
            errors: Vec::new(),
        })
    }

    /// Borrow the underlying session store.
    pub fn store(&self) -> &SessionStore {
        &self.store
    }

    /// Borrow the underlying session store mutably.
    pub fn store_mut(&mut self) -> &mut SessionStore {
        &mut self.store
    }

    /// Consume the adapter and return the underlying session store.
    pub fn into_inner(self) -> SessionStore {
        self.store
    }

    /// Current active session ID. Compression switches this to the child.
    pub fn active_session_id(&self) -> &str {
        &self.active_session_id
    }

    /// Non-fatal persistence errors captured through the trait boundary.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Whether any persistence operation failed.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    fn record_error(&mut self, action: &str, error: impl std::fmt::Display) {
        self.errors.push(format!("{action}: {error}"));
    }
}

impl ConversationStore for StateConversationStore {
    fn persist_message(&mut self, message: &Message) {
        let append = message_to_append(&self.active_session_id, message);
        if let Err(error) = self.store.append_message(append) {
            self.record_error("append_message", error);
        }
    }

    fn persist_compression(&mut self, event: &CompressionEvent) {
        if let Err(error) = self
            .store
            .end_session(&event.parent_session_id, "compression")
        {
            self.record_error("end_parent_for_compression", error);
        }

        let mut child_options = self.options.clone();
        child_options.session_id = event.child_session_id.clone();
        child_options.parent_session_id = Some(event.parent_session_id.clone());
        if let Err(error) = create_session_from_options(&mut self.store, &child_options) {
            self.record_error("create_compression_child", error);
            return;
        }

        self.active_session_id = event.child_session_id.clone();
        self.options = child_options;
    }

    fn persist_token_update(&mut self, budget: &ConversationBudget, model_call_count: u32) {
        let usage = budget.usage;
        let update = TokenUpdate {
            input_tokens: saturating_i64(usage.input_tokens),
            output_tokens: saturating_i64(usage.output_tokens),
            cache_read_tokens: saturating_i64(usage.cache_read_tokens),
            cache_write_tokens: saturating_i64(usage.cache_write_tokens),
            reasoning_tokens: saturating_i64(usage.reasoning_tokens),
            model: self.options.model.clone(),
            api_call_count: i64::from(model_call_count),
            absolute: true,
            ..TokenUpdate::default()
        };
        let session_id = self.active_session_id.clone();
        if let Err(error) = self.store.update_token_counts(&session_id, update) {
            self.record_error("update_token_counts", error);
        }
    }
}

fn create_session_from_options(
    store: &mut SessionStore,
    options: &StateStoreOptions,
) -> Result<String, StateError> {
    let mut session = NewSession::new(&options.session_id, &options.source);
    session.user_id = options.user_id.clone();
    session.model = options.model.clone();
    session.model_config = options.model_config.clone();
    session.system_prompt = options.system_prompt.clone();
    session.parent_session_id = options.parent_session_id.clone();
    store.create_session(session)
}

fn message_to_append(session_id: &str, message: &Message) -> AppendMessage {
    match message {
        Message::System { content } => AppendMessage::new(session_id, "system").text(content),
        Message::User { content } => AppendMessage::new(session_id, "user").text(content),
        Message::Assistant(turn) => {
            let mut append = AppendMessage::new(session_id, "assistant");
            append.content = turn.content.clone().map(Value::String);
            if !turn.tool_calls.is_empty() {
                append.tool_calls = serde_json::to_value(&turn.tool_calls).ok();
            }
            append.reasoning = turn.reasoning.clone();
            append
        }
        Message::Tool(turn) => {
            let mut append = AppendMessage::new(session_id, "tool").text(&turn.content);
            append.tool_call_id = Some(turn.tool_call_id.clone());
            append.tool_name = turn.name.clone();
            append
        }
    }
}

fn saturating_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

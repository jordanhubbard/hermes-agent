//! Conversation messages.
//!
//! `Message` is an externally-tagged enum on `role`, matching the
//! OpenAI-style wire shape that flows through the parity fixtures.

use serde::{Deserialize, Serialize};

use crate::tool::ToolCall;

/// Role on a message. The discriminator for the [`Message`] enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System / instructions message.
    System,
    /// End-user message.
    User,
    /// Model response.
    Assistant,
    /// Tool execution result attached to a prior assistant tool call.
    Tool,
}

/// One message in a conversation.
///
/// Externally tagged on `role` so JSON shapes match the OpenAI-style
/// payloads the Python implementation already produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// System / instructions message.
    System {
        /// Message text.
        content: String,
    },
    /// End-user message.
    User {
        /// Message text.
        content: String,
    },
    /// Assistant turn.
    Assistant(AssistantTurn),
    /// Tool execution result attached to a prior assistant tool call.
    Tool(ToolTurn),
}

/// Assistant turn payload.
///
/// `content` is `None` when the model produced only tool calls. The
/// `reasoning` field carries provider-emitted chain-of-thought text
/// (Anthropic, Codex, DeepSeek-style reasoning models).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AssistantTurn {
    /// Final text the model emitted on this turn.
    ///
    /// Always serialized — `null` (rather than omitted) when the turn
    /// is tool-call-only, matching the OpenAI wire shape the parity
    /// fixtures use.
    #[serde(default)]
    pub content: Option<String>,
    /// Tool calls the model issued on this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Provider-emitted reasoning text, when the model exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

/// Tool message payload — the result of a single tool invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolTurn {
    /// Identifier of the assistant `tool_call` this result answers.
    pub tool_call_id: String,
    /// Optional name of the tool, mirrored from the assistant tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Result payload.
    pub content: String,
    /// Whether the tool succeeded. Optional for back-compat with
    /// payloads that pre-date the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

impl Message {
    /// Return the role of this message without unwrapping the variant.
    pub fn role(&self) -> Role {
        match self {
            Message::System { .. } => Role::System,
            Message::User { .. } => Role::User,
            Message::Assistant(_) => Role::Assistant,
            Message::Tool(_) => Role::Tool,
        }
    }

    /// Convenience constructor for system messages.
    pub fn system(content: impl Into<String>) -> Self {
        Message::System {
            content: content.into(),
        }
    }

    /// Convenience constructor for user messages.
    pub fn user(content: impl Into<String>) -> Self {
        Message::User {
            content: content.into(),
        }
    }

    /// Convenience constructor for an assistant message with no tool calls.
    pub fn assistant_text(content: impl Into<String>) -> Self {
        Message::Assistant(AssistantTurn {
            content: Some(content.into()),
            ..AssistantTurn::default()
        })
    }
}

//! Deterministic compression planning.
//!
//! The real summarizer is provider-backed; this module owns the
//! provider-independent boundary behavior: preserve head/tail turns,
//! insert a summary fallback when the provider returns no summary,
//! count dropped middle messages, and emit lineage metadata for the
//! state layer.

use serde::{Deserialize, Serialize};

use crate::budget::TokenUsage;
use crate::compression::{CompressionEvent, CompressionTrigger};
use crate::message::Message;

const DEFAULT_SUMMARY: &str =
    "Earlier conversation content was compressed; no summary was available.";

/// Options for one compression planning pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionPlanOptions {
    /// Session ID before compression.
    pub parent_session_id: String,
    /// Session ID created for the continuation.
    pub child_session_id: String,
    /// Why compression happened.
    pub trigger: CompressionTrigger,
    /// Number of messages to preserve from the beginning.
    pub head_messages: usize,
    /// Number of messages to preserve from the end.
    pub tail_messages: usize,
    /// Provider-generated summary. Empty/whitespace triggers fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Usage observed at trigger time.
    #[serde(default)]
    pub usage_at_trigger: TokenUsage,
    /// Provider error if the trigger was provider overflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_error: Option<String>,
}

/// Result of planning a compression split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionPlan {
    /// Metadata to persist against the session lineage.
    pub event: CompressionEvent,
    /// Messages that should be written to the child continuation.
    pub retained_messages: Vec<Message>,
}

/// Build a compression plan from a full transcript.
pub fn plan_compression(messages: &[Message], options: CompressionPlanOptions) -> CompressionPlan {
    let len = messages.len();
    let head_end = options.head_messages.min(len);
    let tail_start = len.saturating_sub(options.tail_messages);
    let has_middle = head_end < tail_start;
    let dropped_message_count = if has_middle {
        (tail_start - head_end) as u32
    } else {
        0
    };

    let summary = normalized_summary(options.summary);
    let mut retained = Vec::new();
    retained.extend(messages.iter().take(head_end).cloned());
    if dropped_message_count > 0 {
        retained.push(Message::system(format!(
            "<compressed-context>\n{}\n</compressed-context>",
            summary
        )));
    }
    if tail_start > head_end {
        retained.extend(messages.iter().skip(tail_start).cloned());
    } else {
        retained.extend(messages.iter().skip(head_end).cloned());
    }

    CompressionPlan {
        event: CompressionEvent {
            parent_session_id: options.parent_session_id,
            child_session_id: options.child_session_id,
            trigger: options.trigger,
            dropped_message_count,
            usage_at_trigger: options.usage_at_trigger,
            summary,
            provider_error: options.provider_error,
        },
        retained_messages: retained,
    }
}

fn normalized_summary(summary: Option<String>) -> String {
    match summary {
        Some(text) if !text.trim().is_empty() => text.trim().to_string(),
        _ => DEFAULT_SUMMARY.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        plan_compression, CompressionPlanOptions, CompressionTrigger, Message, TokenUsage,
    };

    #[test]
    fn preserves_head_and_tail_around_summary() {
        let messages = vec![
            Message::system("system"),
            Message::user("u1"),
            Message::assistant_text("a1"),
            Message::user("u2"),
            Message::assistant_text("a2"),
        ];

        let plan = plan_compression(
            &messages,
            CompressionPlanOptions {
                parent_session_id: "parent".to_string(),
                child_session_id: "child".to_string(),
                trigger: CompressionTrigger::ContextLimit,
                head_messages: 1,
                tail_messages: 2,
                summary: Some("middle summary".to_string()),
                usage_at_trigger: TokenUsage {
                    input_tokens: 100,
                    ..TokenUsage::default()
                },
                provider_error: None,
            },
        );

        assert_eq!(plan.event.parent_session_id, "parent");
        assert_eq!(plan.event.child_session_id, "child");
        assert_eq!(plan.event.dropped_message_count, 2);
        assert_eq!(plan.event.summary, "middle summary");
        assert_eq!(plan.retained_messages.len(), 4);
        assert_eq!(plan.retained_messages[0], Message::system("system"));
        assert!(
            matches!(&plan.retained_messages[1], Message::System { content } if content.contains("middle summary"))
        );
        assert_eq!(plan.retained_messages[2], Message::user("u2"));
        assert_eq!(plan.retained_messages[3], Message::assistant_text("a2"));
    }

    #[test]
    fn blank_summary_uses_fallback() {
        let messages = vec![Message::user("u1"), Message::assistant_text("a1")];
        let plan = plan_compression(
            &messages,
            CompressionPlanOptions {
                parent_session_id: "p".to_string(),
                child_session_id: "c".to_string(),
                trigger: CompressionTrigger::ProviderOverflow,
                head_messages: 0,
                tail_messages: 1,
                summary: Some("  ".to_string()),
                usage_at_trigger: TokenUsage::default(),
                provider_error: Some("context exceeded".to_string()),
            },
        );
        assert!(plan.event.summary.contains("no summary was available"));
        assert_eq!(
            plan.event.provider_error.as_deref(),
            Some("context exceeded")
        );
    }

    #[test]
    fn no_overlap_duplicate_when_transcript_is_short() {
        let messages = vec![Message::user("u1"), Message::assistant_text("a1")];
        let plan = plan_compression(
            &messages,
            CompressionPlanOptions {
                parent_session_id: "p".to_string(),
                child_session_id: "c".to_string(),
                trigger: CompressionTrigger::UserRequested,
                head_messages: 5,
                tail_messages: 5,
                summary: Some("unused".to_string()),
                usage_at_trigger: TokenUsage::default(),
                provider_error: None,
            },
        );
        assert_eq!(plan.event.dropped_message_count, 0);
        assert_eq!(plan.retained_messages, messages);
    }
}

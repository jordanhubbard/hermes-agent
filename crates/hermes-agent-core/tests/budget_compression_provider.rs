//! Tests for the budget, compression, provider, and outcome types
//! added to close hermes-1oa.1.

use std::collections::BTreeMap;

use hermes_agent_core::{
    ApiMode, CompressionEvent, CompressionTrigger, ConversationBudget, ConversationOutcome,
    ConversationResult, InterruptKind, LineageTip, ProviderRouting, TokenUsage, TurnCost,
};
use serde_json::{json, Value};

// ─── budget ──────────────────────────────────────────────────────────────

#[test]
fn token_usage_default_is_all_zero_and_round_trips() {
    let usage = TokenUsage::default();
    let v = serde_json::to_value(&usage).unwrap();
    // Default values still serialize since the fields aren't optional.
    assert_eq!(v["input_tokens"], 0);
    assert_eq!(v["output_tokens"], 0);
    assert_eq!(v["reasoning_tokens"], 0);
    let back: TokenUsage = serde_json::from_value(v).unwrap();
    assert_eq!(back, usage);
}

#[test]
fn token_usage_add_saturates() {
    let mut a = TokenUsage {
        input_tokens: u64::MAX - 1,
        ..Default::default()
    };
    let b = TokenUsage {
        input_tokens: 10,
        ..Default::default()
    };
    a.add(&b);
    assert_eq!(a.input_tokens, u64::MAX, "must saturate, not wrap");
}

#[test]
fn token_usage_total_in_context() {
    let usage = TokenUsage {
        input_tokens: 100,
        output_tokens: 50,
        cache_read_tokens: 25,
        cache_write_tokens: 7, // not in context
        reasoning_tokens: 8,
    };
    assert_eq!(usage.total_in_context(), 100 + 50 + 25 + 8);
}

#[test]
fn turn_cost_omits_none_fields_on_serialize() {
    let cost = TurnCost::default();
    let v = serde_json::to_value(&cost).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 0);
}

#[test]
fn conversation_budget_turn_and_context_caps() {
    let mut b = ConversationBudget {
        max_turns: Some(5),
        model_context_limit: Some(1000),
        ..Default::default()
    };
    assert!(!b.turns_exhausted());
    assert!(!b.context_exhausted());
    b.turn_count = 5;
    assert!(b.turns_exhausted());
    b.usage = TokenUsage {
        input_tokens: 1500,
        ..Default::default()
    };
    assert!(b.context_exhausted());
}

#[test]
fn conversation_budget_no_caps_means_never_exhausted() {
    let b = ConversationBudget {
        max_turns: None,
        model_context_limit: None,
        turn_count: u32::MAX,
        usage: TokenUsage {
            input_tokens: u64::MAX,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(!b.turns_exhausted());
    assert!(!b.context_exhausted());
}

// ─── compression ────────────────────────────────────────────────────────

#[test]
fn compression_event_round_trips_with_provider_overflow() {
    let event = CompressionEvent {
        parent_session_id: "p".into(),
        child_session_id: "c".into(),
        trigger: CompressionTrigger::ProviderOverflow,
        dropped_message_count: 12,
        usage_at_trigger: TokenUsage {
            input_tokens: 90_000,
            ..Default::default()
        },
        summary: "user asked for X".into(),
        provider_error: Some("context_length_exceeded: 130000 > 128000".into()),
    };
    let v = serde_json::to_value(&event).unwrap();
    assert_eq!(v["trigger"], "provider_overflow");
    assert_eq!(v["dropped_message_count"], 12);
    assert!(v.get("provider_error").is_some());

    let back: CompressionEvent = serde_json::from_value(v).unwrap();
    assert_eq!(back, event);
}

#[test]
fn compression_event_omits_none_provider_error() {
    let event = CompressionEvent {
        parent_session_id: "p".into(),
        child_session_id: "c".into(),
        trigger: CompressionTrigger::UserRequested,
        dropped_message_count: 3,
        usage_at_trigger: TokenUsage::default(),
        summary: "x".into(),
        provider_error: None,
    };
    let v = serde_json::to_value(&event).unwrap();
    assert!(v.get("provider_error").is_none());
}

#[test]
fn lineage_tip_round_trips() {
    let tip = LineageTip {
        tip_session_id: "tip".into(),
        root_session_id: "root".into(),
        depth: 3,
    };
    let v = serde_json::to_value(&tip).unwrap();
    let back: LineageTip = serde_json::from_value(v).unwrap();
    assert_eq!(back, tip);
}

// ─── provider ───────────────────────────────────────────────────────────

#[test]
fn provider_routing_round_trips() {
    let mut headers = BTreeMap::new();
    headers.insert("anthropic-version".into(), "2023-06-01".into());
    headers.insert("authorization".into(), "Bearer x".into());

    let routing = ProviderRouting {
        provider: "anthropic".into(),
        model: "claude-sonnet-4-6".into(),
        base_url: Some("https://api.anthropic.com".into()),
        api_mode: ApiMode::Anthropic,
        extra_headers: headers,
        provider_options: Some(json!({"max_tokens": 4096})),
    };

    let v: Value = serde_json::to_value(&routing).unwrap();
    assert_eq!(v["provider"], "anthropic");
    assert_eq!(v["api_mode"], "anthropic");
    assert_eq!(v["extra_headers"]["anthropic-version"], "2023-06-01");

    let back: ProviderRouting = serde_json::from_value(v).unwrap();
    assert_eq!(back, routing);
}

#[test]
fn provider_routing_omits_empty_optional_fields() {
    let routing = ProviderRouting {
        provider: "openai".into(),
        model: "gpt-4o".into(),
        base_url: None,
        api_mode: ApiMode::ChatCompletions,
        extra_headers: BTreeMap::new(),
        provider_options: None,
    };
    let v = serde_json::to_value(&routing).unwrap();
    assert!(v.get("base_url").is_none());
    assert!(v.get("extra_headers").is_none());
    assert!(v.get("provider_options").is_none());
}

#[test]
fn api_mode_round_trips_each_variant() {
    for mode in [
        ApiMode::ChatCompletions,
        ApiMode::Responses,
        ApiMode::Anthropic,
        ApiMode::Bedrock,
        ApiMode::OpenAiCompat,
    ] {
        let v = serde_json::to_value(mode).unwrap();
        let back: ApiMode = serde_json::from_value(v).unwrap();
        assert_eq!(back, mode);
    }
}

// ─── outcome ────────────────────────────────────────────────────────────

#[test]
fn conversation_outcome_completed_round_trips() {
    let outcome = ConversationOutcome::Completed {
        final_message: "done".into(),
    };
    let v = serde_json::to_value(&outcome).unwrap();
    assert_eq!(v["kind"], "completed");
    assert_eq!(v["final_message"], "done");
    let back: ConversationOutcome = serde_json::from_value(v).unwrap();
    assert_eq!(back, outcome);
}

#[test]
fn conversation_outcome_interrupted_carries_kind() {
    let outcome = ConversationOutcome::Interrupted {
        reason: InterruptKind::User,
        detail: Some("Ctrl+C".into()),
    };
    let v = serde_json::to_value(&outcome).unwrap();
    assert_eq!(v["kind"], "interrupted");
    assert_eq!(v["reason"], "user");
    assert_eq!(v["detail"], "Ctrl+C");
}

#[test]
fn conversation_outcome_context_overflow_has_no_payload() {
    let outcome = ConversationOutcome::ContextOverflow;
    let v = serde_json::to_value(&outcome).unwrap();
    assert_eq!(v["kind"], "context_overflow");
    assert_eq!(v.as_object().unwrap().len(), 1);
}

#[test]
fn conversation_outcome_tool_loop_carries_tool_name() {
    let outcome = ConversationOutcome::ToolLoop {
        tool_name: "terminal".into(),
    };
    let v = serde_json::to_value(&outcome).unwrap();
    assert_eq!(v["kind"], "tool_loop");
    assert_eq!(v["tool_name"], "terminal");
}

#[test]
fn conversation_result_bundles_outcome_and_budget() {
    let result = ConversationResult {
        outcome: ConversationOutcome::Completed {
            final_message: "ok".into(),
        },
        budget: ConversationBudget {
            usage: TokenUsage {
                input_tokens: 7,
                output_tokens: 3,
                ..Default::default()
            },
            turn_count: 2,
            ..Default::default()
        },
    };
    let v = serde_json::to_value(&result).unwrap();
    assert_eq!(v["outcome"]["kind"], "completed");
    assert_eq!(v["budget"]["turn_count"], 2);
    let back: ConversationResult = serde_json::from_value(v).unwrap();
    assert_eq!(back, result);
}

#[test]
fn interrupt_kind_round_trips_each_variant() {
    for kind in [
        InterruptKind::User,
        InterruptKind::SlashStop,
        InterruptKind::MaxTurns,
        InterruptKind::ToolTimeout,
        InterruptKind::ApprovalDenied,
        InterruptKind::External,
    ] {
        let v = serde_json::to_value(kind).unwrap();
        let back: InterruptKind = serde_json::from_value(v).unwrap();
        assert_eq!(back, kind);
    }
}

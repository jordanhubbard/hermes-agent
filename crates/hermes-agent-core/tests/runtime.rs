use std::collections::VecDeque;

use hermes_agent_core::{
    run_agent_runtime, AgentRuntimeConfig, AssistantTurn, ConversationBudget, ConversationOutcome,
    ConversationStore, InterruptKind, Message, ModelClient, ParsedProviderResponse,
    ProviderErrorClass, ProviderHttpError, ProviderRequestOptions, RuntimeCompressionOptions,
    RuntimeDeps, RuntimeHooks, TokenUsage, ToolCall, ToolDefinition, ToolDispatcher, ToolResult,
};
use serde_json::json;

#[derive(Default)]
struct FakeModel {
    responses: VecDeque<Result<ParsedProviderResponse, ProviderHttpError>>,
    seen_message_counts: Vec<usize>,
}

impl FakeModel {
    fn with(responses: Vec<Result<ParsedProviderResponse, ProviderHttpError>>) -> Self {
        Self {
            responses: responses.into(),
            seen_message_counts: Vec::new(),
        }
    }
}

impl ModelClient for FakeModel {
    fn call(
        &mut self,
        messages: &[Message],
        _tools: &[ToolDefinition],
        _options: &ProviderRequestOptions,
    ) -> Result<ParsedProviderResponse, ProviderHttpError> {
        self.seen_message_counts.push(messages.len());
        self.responses
            .pop_front()
            .expect("fake model response exists")
    }
}

#[derive(Default)]
struct FakeTools {
    calls: Vec<ToolCall>,
}

impl ToolDispatcher for FakeTools {
    fn dispatch(&mut self, call: &ToolCall) -> ToolResult {
        self.calls.push(call.clone());
        ToolResult {
            call_id: call.id.clone(),
            ok: true,
            content: format!("tool-result:{}", call.name),
        }
    }
}

#[derive(Default)]
struct RecordingStore {
    messages: Vec<Message>,
    compressions: Vec<String>,
}

impl ConversationStore for RecordingStore {
    fn persist_message(&mut self, message: &Message) {
        self.messages.push(message.clone());
    }

    fn persist_compression(&mut self, event: &hermes_agent_core::CompressionEvent) {
        self.compressions.push(event.summary.clone());
    }
}

#[derive(Default)]
struct RecordingHooks {
    events: Vec<String>,
}

impl RuntimeHooks for RecordingHooks {
    fn on_session_start(&mut self) {
        self.events.push("start".to_string());
    }

    fn on_model_call(&mut self, call_index: u32, message_count: usize) {
        self.events
            .push(format!("model:{call_index}:{message_count}"));
    }

    fn on_provider_error(&mut self, error: &ProviderHttpError) {
        self.events
            .push(format!("provider_error:{:?}", error.class));
    }

    fn on_tool_call(&mut self, call: &ToolCall) {
        self.events.push(format!("tool:{}", call.name));
    }

    fn on_session_end(&mut self, outcome: &ConversationOutcome) {
        self.events.push(format!("end:{}", outcome_kind(outcome)));
    }
}

fn parsed(assistant: AssistantTurn, usage: TokenUsage) -> ParsedProviderResponse {
    ParsedProviderResponse {
        assistant,
        usage,
        finish_reason: None,
    }
}

fn provider_error(class: ProviderErrorClass, message: &str) -> ProviderHttpError {
    ProviderHttpError {
        url: Some("http://mock/v1/chat/completions".to_string()),
        status: Some(429),
        class,
        message: message.to_string(),
    }
}

fn call(id: &str, name: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments: json!({"path": "README.md"}),
    }
}

fn deps<'a>(
    model: &'a mut dyn ModelClient,
    fallback_model: Option<&'a mut dyn ModelClient>,
    tools: &'a mut dyn ToolDispatcher,
    store: &'a mut dyn ConversationStore,
    hooks: &'a mut dyn RuntimeHooks,
) -> RuntimeDeps<'a> {
    RuntimeDeps {
        model,
        fallback_model,
        tools,
        store,
        hooks,
    }
}

#[test]
fn runtime_dispatches_tools_persists_messages_and_fires_hooks() {
    let tool_call = call("call_1", "read_file");
    let mut model = FakeModel::with(vec![
        Ok(parsed(
            AssistantTurn {
                content: None,
                tool_calls: vec![tool_call],
                reasoning: Some("need file".to_string()),
            },
            TokenUsage {
                input_tokens: 10,
                output_tokens: 2,
                ..TokenUsage::default()
            },
        )),
        Ok(parsed(
            AssistantTurn {
                content: Some("done".to_string()),
                ..AssistantTurn::default()
            },
            TokenUsage {
                input_tokens: 4,
                output_tokens: 1,
                ..TokenUsage::default()
            },
        )),
    ]);
    let mut tools = FakeTools::default();
    let mut store = RecordingStore::default();
    let mut hooks = RecordingHooks::default();

    let result = run_agent_runtime(
        vec![Message::user("read")],
        &[],
        AgentRuntimeConfig::default(),
        deps(&mut model, None, &mut tools, &mut store, &mut hooks),
    );

    assert_eq!(
        result.outcome,
        ConversationOutcome::Completed {
            final_message: "done".to_string()
        }
    );
    assert_eq!(result.model_call_count, 2);
    assert_eq!(result.tool_iteration_count, 1);
    assert_eq!(result.tool_call_count, 1);
    assert_eq!(result.budget.usage.input_tokens, 14);
    assert_eq!(tools.calls[0].name, "read_file");
    assert_eq!(store.messages.len(), 4);
    assert_eq!(
        hooks.events,
        vec![
            "start",
            "model:0:1",
            "tool:read_file",
            "model:1:3",
            "end:completed"
        ]
    );
}

#[test]
fn runtime_uses_fallback_for_retryable_provider_error() {
    let mut primary = FakeModel::with(vec![Err(provider_error(
        ProviderErrorClass::RateLimit,
        "rate limit",
    ))]);
    let mut fallback = FakeModel::with(vec![Ok(parsed(
        AssistantTurn {
            content: Some("fallback ok".to_string()),
            ..AssistantTurn::default()
        },
        TokenUsage {
            input_tokens: 1,
            output_tokens: 2,
            ..TokenUsage::default()
        },
    ))]);
    let mut tools = FakeTools::default();
    let mut store = RecordingStore::default();
    let mut hooks = RecordingHooks::default();

    let result = run_agent_runtime(
        vec![Message::user("hello")],
        &[],
        AgentRuntimeConfig::default(),
        deps(
            &mut primary,
            Some(&mut fallback),
            &mut tools,
            &mut store,
            &mut hooks,
        ),
    );

    assert!(result.fallback_used);
    assert_eq!(result.model_call_count, 2);
    assert_eq!(
        result.outcome,
        ConversationOutcome::Completed {
            final_message: "fallback ok".to_string()
        }
    );
    assert!(hooks
        .events
        .contains(&"provider_error:RateLimit".to_string()));
}

#[test]
fn runtime_interrupts_before_provider_call() {
    let mut model = FakeModel::default();
    let mut tools = FakeTools::default();
    let mut store = RecordingStore::default();
    let mut hooks = RecordingHooks::default();

    let result = run_agent_runtime(
        vec![Message::user("hello")],
        &[],
        AgentRuntimeConfig {
            interrupt_before_model_call: Some(0),
            ..AgentRuntimeConfig::default()
        },
        deps(&mut model, None, &mut tools, &mut store, &mut hooks),
    );

    assert_eq!(result.model_call_count, 0);
    assert_eq!(
        result.outcome,
        ConversationOutcome::Interrupted {
            reason: InterruptKind::User,
            detail: Some("interrupt flag set before model call".to_string())
        }
    );
}

#[test]
fn runtime_compresses_context_before_provider_call() {
    let mut model = FakeModel::with(vec![Ok(parsed(
        AssistantTurn {
            content: Some("after compression".to_string()),
            ..AssistantTurn::default()
        },
        TokenUsage::default(),
    ))]);
    let mut tools = FakeTools::default();
    let mut store = RecordingStore::default();
    let mut hooks = RecordingHooks::default();

    let result = run_agent_runtime(
        vec![
            Message::system("system"),
            Message::user("u1"),
            Message::assistant_text("a1"),
            Message::user("u2"),
            Message::assistant_text("a2"),
        ],
        &[],
        AgentRuntimeConfig {
            budget: ConversationBudget {
                usage: TokenUsage {
                    input_tokens: 100,
                    ..TokenUsage::default()
                },
                model_context_limit: Some(10),
                ..ConversationBudget::default()
            },
            compression: Some(RuntimeCompressionOptions {
                parent_session_id: "parent".to_string(),
                child_session_id: "child".to_string(),
                head_messages: 1,
                tail_messages: 1,
                summary: Some("compressed middle".to_string()),
            }),
            ..AgentRuntimeConfig::default()
        },
        deps(&mut model, None, &mut tools, &mut store, &mut hooks),
    );

    assert_eq!(result.compression_events.len(), 1);
    assert_eq!(result.compression_events[0].dropped_message_count, 3);
    assert_eq!(store.compressions, vec!["compressed middle"]);
    assert_eq!(model.seen_message_counts, vec![3]);
    assert!(matches!(result.messages[1], Message::System { .. }));
}

fn outcome_kind(outcome: &ConversationOutcome) -> &'static str {
    match outcome {
        ConversationOutcome::Completed { .. } => "completed",
        ConversationOutcome::Interrupted { .. } => "interrupted",
        ConversationOutcome::ProviderError { .. } => "provider_error",
        ConversationOutcome::ContextOverflow => "context_overflow",
        ConversationOutcome::ToolLoop { .. } => "tool_loop",
    }
}

use hermes_agent_core::{
    run_agent_runtime, AgentRuntimeConfig, AssistantTurn, ConversationBudget, Message, ModelClient,
    ParsedProviderResponse, ProviderHttpError, ProviderRequestOptions, RuntimeCompressionOptions,
    RuntimeDeps, RuntimeHooks, StateConversationStore, StateStoreOptions, TokenUsage, ToolCall,
    ToolDefinition, ToolDispatcher, ToolResult,
};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct FakeModel {
    responses: VecDeque<ParsedProviderResponse>,
}

impl FakeModel {
    fn new(responses: Vec<ParsedProviderResponse>) -> Self {
        Self {
            responses: responses.into(),
        }
    }
}

impl ModelClient for FakeModel {
    fn call(
        &mut self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _options: &ProviderRequestOptions,
    ) -> Result<ParsedProviderResponse, ProviderHttpError> {
        Ok(self.responses.pop_front().expect("fake response exists"))
    }
}

struct FakeTools;

impl ToolDispatcher for FakeTools {
    fn dispatch(&mut self, call: &ToolCall) -> ToolResult {
        ToolResult {
            call_id: call.id.clone(),
            ok: true,
            content: json!({"ok": true, "tool": call.name}).to_string(),
        }
    }
}

#[derive(Default)]
struct Hooks;

impl RuntimeHooks for Hooks {}

fn parsed(assistant: AssistantTurn, usage: TokenUsage) -> ParsedProviderResponse {
    ParsedProviderResponse {
        assistant,
        usage,
        finish_reason: None,
    }
}

fn temp_db_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("hermes-agent-core-{name}-{nanos}.sqlite"))
}

fn deps<'a>(
    model: &'a mut dyn ModelClient,
    tools: &'a mut dyn ToolDispatcher,
    store: &'a mut StateConversationStore,
    hooks: &'a mut dyn RuntimeHooks,
) -> RuntimeDeps<'a> {
    RuntimeDeps {
        model,
        fallback_model: None,
        tools,
        store,
        hooks,
    }
}

#[test]
fn state_store_persists_runtime_messages_tool_calls_reasoning_and_tokens() {
    let db_path = temp_db_path("messages");
    let tool_call = ToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "README.md"}),
    };
    let mut model = FakeModel::new(vec![
        parsed(
            AssistantTurn {
                content: None,
                tool_calls: vec![tool_call],
                reasoning: Some("need the file".to_string()),
            },
            TokenUsage {
                input_tokens: 11,
                output_tokens: 3,
                cache_read_tokens: 2,
                reasoning_tokens: 1,
                ..TokenUsage::default()
            },
        ),
        parsed(
            AssistantTurn {
                content: Some("done".to_string()),
                ..AssistantTurn::default()
            },
            TokenUsage {
                input_tokens: 5,
                output_tokens: 7,
                ..TokenUsage::default()
            },
        ),
    ]);
    let mut tools = FakeTools;
    let mut hooks = Hooks;
    let mut state_store = StateConversationStore::open(
        &db_path,
        StateStoreOptions::new("session-1", "cli")
            .model("test-model")
            .system_prompt("system prompt"),
    )
    .unwrap();

    let result = run_agent_runtime(
        vec![Message::system("system"), Message::user("read README")],
        &[],
        AgentRuntimeConfig::default(),
        deps(&mut model, &mut tools, &mut state_store, &mut hooks),
    );

    assert!(!state_store.has_errors(), "{:?}", state_store.errors());
    assert_eq!(result.model_call_count, 2);

    let session = state_store
        .store()
        .get_session("session-1")
        .unwrap()
        .unwrap();
    assert_eq!(session.model.as_deref(), Some("test-model"));
    assert_eq!(session.system_prompt.as_deref(), Some("system prompt"));
    assert_eq!(session.message_count, 5);
    assert_eq!(session.tool_call_count, 1);
    assert_eq!(session.input_tokens, 16);
    assert_eq!(session.output_tokens, 10);
    assert_eq!(session.cache_read_tokens, 2);
    assert_eq!(session.reasoning_tokens, 1);
    assert_eq!(session.api_call_count, 2);

    let messages = state_store.store().get_messages("session-1").unwrap();
    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0].role, "system");
    assert_eq!(
        messages[2].tool_calls.as_ref().unwrap()[0]["name"],
        Value::String("read_file".to_string())
    );
    assert_eq!(messages[2].reasoning.as_deref(), Some("need the file"));
    assert_eq!(messages[3].role, "tool");
    assert_eq!(messages[3].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(messages[3].tool_name.as_deref(), Some("read_file"));
    assert_eq!(
        messages[4].content.as_ref(),
        Some(&Value::String("done".to_string()))
    );
}

#[test]
fn state_store_persists_compression_lineage_to_child_session() {
    let db_path = temp_db_path("compression");
    let mut model = FakeModel::new(vec![parsed(
        AssistantTurn {
            content: Some("after compression".to_string()),
            ..AssistantTurn::default()
        },
        TokenUsage {
            input_tokens: 1,
            output_tokens: 1,
            ..TokenUsage::default()
        },
    )]);
    let mut tools = FakeTools;
    let mut hooks = Hooks;
    let mut state_store = StateConversationStore::open(
        &db_path,
        StateStoreOptions::new("parent", "cli").model("test-model"),
    )
    .unwrap();

    run_agent_runtime(
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
        deps(&mut model, &mut tools, &mut state_store, &mut hooks),
    );

    assert!(!state_store.has_errors(), "{:?}", state_store.errors());
    assert_eq!(state_store.active_session_id(), "child");

    let parent = state_store.store().get_session("parent").unwrap().unwrap();
    let child = state_store.store().get_session("child").unwrap().unwrap();
    assert_eq!(parent.end_reason.as_deref(), Some("compression"));
    assert_eq!(child.parent_session_id.as_deref(), Some("parent"));
    assert_eq!(
        state_store.store().get_compression_tip("parent").unwrap(),
        "child"
    );

    let parent_messages = state_store.store().get_messages("parent").unwrap();
    let child_messages = state_store.store().get_messages("child").unwrap();
    assert_eq!(parent_messages.len(), 5);
    assert_eq!(child_messages.len(), 4);
    assert_eq!(child_messages[0].role, "system");
    assert!(child_messages[1]
        .content
        .as_ref()
        .and_then(Value::as_str)
        .unwrap()
        .contains("compressed middle"));
    assert_eq!(
        child_messages[3].content.as_ref(),
        Some(&Value::String("after compression".to_string()))
    );
}

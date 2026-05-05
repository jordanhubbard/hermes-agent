//! Verify that the agent-core types round-trip the JSON shapes used by
//! the parity fixtures.
//!
//! The fixtures live at `tests/parity/fixtures/*.json` (relative to the
//! repo root). These tests load the fixture inputs, deserialize the
//! pieces this crate models, re-serialize them, and require structural
//! equality with the original JSON.

use std::path::PathBuf;

use hermes_agent_core::{
    AssistantTurn, Message, Role, ToolCall, ToolDefinition, ToolFunction, ToolResult,
};
use serde_json::{json, Value};

fn fixture_path(name: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("crate dir has two parents")
        .join("tests/parity/fixtures")
        .join(name)
}

fn load_fixture(name: &str) -> Value {
    let path = fixture_path(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture is valid JSON")
}

#[test]
fn message_role_helper_matches_variant() {
    assert_eq!(Message::system("x").role(), Role::System);
    assert_eq!(Message::user("x").role(), Role::User);
    assert_eq!(Message::assistant_text("x").role(), Role::Assistant);
}

#[test]
fn assistant_text_message_round_trips() {
    let msg = Message::assistant_text("The capital of France is Paris.");
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["role"], "assistant");
    assert_eq!(value["content"], "The capital of France is Paris.");
    assert!(
        value.get("tool_calls").is_none(),
        "empty tool_calls should not serialize"
    );
    assert!(
        value.get("reasoning").is_none(),
        "missing reasoning should not serialize"
    );

    let back: Message = serde_json::from_value(value).unwrap();
    assert_eq!(back, msg);
}

#[test]
fn assistant_turn_serializes_null_content() {
    // Default AssistantTurn carries content=None, no tool_calls, no
    // reasoning. The wire shape must keep `content: null` because the
    // OpenAI spec (and the parity fixtures) treat `content: null` as
    // "tool-call-only turn" rather than "no field".
    let turn = AssistantTurn::default();
    let v = serde_json::to_value(&turn).unwrap();
    let obj = v.as_object().unwrap();
    assert_eq!(obj.len(), 1, "only content should serialize: {v:?}");
    assert!(obj.get("content").unwrap().is_null());
}

#[test]
fn assistant_turn_with_tool_call_round_trips() {
    // Mirror the canned response in fixtures/02_single_tool_call.json.
    let payload = json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [
            {
                "id": "call_1",
                "name": "read_file",
                "arguments": {"path": "README.md"}
            }
        ]
    });
    let msg: Message = serde_json::from_value(payload.clone()).unwrap();
    match &msg {
        Message::Assistant(turn) => {
            assert!(turn.content.is_none());
            assert_eq!(turn.tool_calls.len(), 1);
            assert_eq!(turn.tool_calls[0].name, "read_file");
            assert_eq!(turn.tool_calls[0].id, "call_1");
            assert_eq!(turn.tool_calls[0].arguments["path"], "README.md");
        }
        _ => panic!("expected Assistant"),
    }
    let re = serde_json::to_value(&msg).unwrap();
    assert_eq!(re, payload);
}

#[test]
fn assistant_turn_with_reasoning_field_round_trips() {
    // Mirror fixtures/04_reasoning_field.json.
    let payload = json!({
        "role": "assistant",
        "content": "17 * 23 = 391",
        "reasoning": "17 * 23 = 17 * (20 + 3) = 340 + 51 = 391."
    });
    let msg: Message = serde_json::from_value(payload.clone()).unwrap();
    match &msg {
        Message::Assistant(turn) => {
            assert_eq!(turn.content.as_deref(), Some("17 * 23 = 391"));
            assert!(turn.reasoning.is_some());
        }
        _ => panic!("expected Assistant"),
    }
    let re = serde_json::to_value(&msg).unwrap();
    assert_eq!(re, payload);
}

#[test]
fn tool_message_round_trips() {
    let payload = json!({
        "role": "tool",
        "tool_call_id": "call_1",
        "name": "read_file",
        "content": "Hermes Agent\nA local-first AI agent.",
        "ok": true
    });
    let msg: Message = serde_json::from_value(payload.clone()).unwrap();
    match &msg {
        Message::Tool(t) => {
            assert_eq!(t.tool_call_id, "call_1");
            assert_eq!(t.name.as_deref(), Some("read_file"));
            assert_eq!(t.ok, Some(true));
        }
        _ => panic!("expected Tool"),
    }
    assert_eq!(serde_json::to_value(&msg).unwrap(), payload);
}

#[test]
fn tool_definition_from_fixture_02_round_trips() {
    let fixture = load_fixture("02_single_tool_call.json");
    let defs: Vec<ToolDefinition> =
        serde_json::from_value(fixture["inputs"]["tool_definitions"].clone()).unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].kind, "function");
    assert_eq!(defs[0].function.name, "read_file");
    let re = serde_json::to_value(&defs).unwrap();
    assert_eq!(re, fixture["inputs"]["tool_definitions"]);
}

#[test]
fn tool_call_from_fixture_03_round_trips() {
    let fixture = load_fixture("03_multi_turn_tool_use.json");
    let calls: Vec<ToolCall> = serde_json::from_value(
        fixture["inputs"]["canned_model_responses"][0]["tool_calls"].clone(),
    )
    .unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "search_files");
    assert_eq!(calls[0].arguments["pattern"], "foo");
}

#[test]
fn tool_result_constructs_and_serializes() {
    let r = ToolResult {
        call_id: "call_1".into(),
        ok: false,
        content: "FileNotFoundError: missing.txt".into(),
    };
    let v = serde_json::to_value(&r).unwrap();
    assert_eq!(v["call_id"], "call_1");
    assert_eq!(v["ok"], false);
    let back: ToolResult = serde_json::from_value(v).unwrap();
    assert_eq!(back, r);
}

#[test]
fn tool_function_definition_constructs() {
    let def = ToolDefinition {
        kind: "function".into(),
        function: ToolFunction {
            name: "read_file".into(),
            description: Some("Read a file from disk.".into()),
            parameters: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
    };
    let v = serde_json::to_value(&def).unwrap();
    assert_eq!(v["type"], "function");
    assert_eq!(v["function"]["name"], "read_file");
    let back: ToolDefinition = serde_json::from_value(v).unwrap();
    assert_eq!(back, def);
}

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value};

pub mod clarify;
pub mod handlers;
pub mod homeassistant;
pub mod memory;
pub mod safety;
pub mod session_search;
pub mod skill_manage;
pub mod skills;
pub mod todo;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolsetDef {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RegistryFixture {
    pub core_tools: Vec<String>,
    pub toolsets: BTreeMap<String, ToolsetDef>,
    pub legacy_toolsets: BTreeMap<String, Vec<String>>,
    pub schemas: Vec<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolRegistrySnapshot {
    pub core_tools: Vec<String>,
    pub all_toolsets: Vec<String>,
    pub registered_tool_names: Vec<String>,
    pub tool_to_toolset: BTreeMap<String, String>,
    pub resolved_toolsets: BTreeMap<String, Vec<String>>,
    pub schema_names: BTreeMap<String, Vec<String>>,
    pub schemas: BTreeMap<String, Vec<Value>>,
    pub cache_isolation: CacheIsolationSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CacheIsolationSnapshot {
    pub default_names_before: Vec<String>,
    pub file_names: Vec<String>,
    pub default_names_after: Vec<String>,
    pub cache_keys_distinct: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DispatchParitySnapshot {
    pub registry_dispatch: BTreeMap<String, RegistryDispatchCaseSnapshot>,
    pub handle_function_call: BTreeMap<String, FunctionCallCaseSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RegistryDispatchCaseSnapshot {
    pub result: String,
    pub parsed_result: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FunctionCallCaseSnapshot {
    pub args_after_coercion: Value,
    pub dispatches: Vec<DispatchInvocation>,
    pub hook_events: Vec<HookObservation>,
    pub notifications: Vec<String>,
    pub parsed_result: Option<Value>,
    pub result: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DispatchInvocation {
    pub tool_name: String,
    pub args: Value,
    pub task_id: Option<String>,
    pub user_task: Option<String>,
    pub enabled_tools: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HookObservation {
    pub hook: String,
    pub tool_name: String,
    pub args: Value,
    pub result: Option<String>,
    pub task_id: String,
    pub session_id: String,
    pub tool_call_id: String,
    pub duration_ms: Option<String>,
}

pub fn fixture() -> RegistryFixture {
    serde_json::from_str(include_str!("tool_registry_snapshot.json"))
        .expect("tool registry fixture is valid JSON")
}

pub fn tool_registry_snapshot() -> ToolRegistrySnapshot {
    let fixture = fixture();
    let sample_toolsets = [
        "web",
        "file",
        "browser",
        "debugging",
        "hermes-cli",
        "hermes-discord",
        "hermes-feishu",
        "hermes-gateway",
        "terminal_tools",
        "all",
        "unknown",
    ];
    let schema_cases = [
        ("default", None, Vec::<&str>::new()),
        ("hermes-cli", Some(vec!["hermes-cli"]), Vec::<&str>::new()),
        ("file", Some(vec!["file"]), Vec::<&str>::new()),
        ("browser", Some(vec!["browser"]), Vec::<&str>::new()),
        ("debugging_no_file", Some(vec!["debugging"]), vec!["file"]),
        (
            "discord_without_browser",
            Some(vec!["hermes-discord"]),
            vec!["browser"],
        ),
        (
            "legacy_file_tools",
            Some(vec!["file_tools"]),
            Vec::<&str>::new(),
        ),
    ];
    let mut resolved_toolsets = BTreeMap::new();
    for name in sample_toolsets {
        resolved_toolsets.insert(
            name.to_string(),
            resolve_toolset_with_fixture(&fixture, name),
        );
    }
    let mut schema_names = BTreeMap::new();
    let mut schemas = BTreeMap::new();
    for (label, enabled, disabled) in schema_cases {
        let enabled_owned = enabled.map(|items| {
            items
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
        });
        let disabled_owned = disabled
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        let selected = get_tool_definitions_with_fixture(
            &fixture,
            enabled_owned.as_deref(),
            if disabled_owned.is_empty() {
                None
            } else {
                Some(disabled_owned.as_slice())
            },
        );
        schema_names.insert(label.to_string(), schema_names_from_values(&selected));
        schemas.insert(label.to_string(), selected);
    }

    let default_before = get_tool_definitions_with_fixture(&fixture, None, None);
    let file = get_tool_definitions_with_fixture(&fixture, Some(&["file".to_string()]), None);
    let default_after = get_tool_definitions_with_fixture(&fixture, None, None);

    ToolRegistrySnapshot {
        core_tools: fixture.core_tools.clone(),
        all_toolsets: all_toolsets_with_fixture(&fixture),
        registered_tool_names: registered_tool_names_with_fixture(&fixture),
        tool_to_toolset: tool_to_toolset_with_fixture(&fixture),
        resolved_toolsets,
        schema_names,
        schemas,
        cache_isolation: CacheIsolationSnapshot {
            default_names_before: schema_names_from_values(&default_before),
            file_names: schema_names_from_values(&file),
            default_names_after: schema_names_from_values(&default_after),
            cache_keys_distinct: schema_names_from_values(&default_before)
                != schema_names_from_values(&file),
        },
    }
}

pub fn all_toolsets() -> Vec<String> {
    all_toolsets_with_fixture(&fixture())
}

pub fn resolve_toolset(name: &str) -> Vec<String> {
    resolve_toolset_with_fixture(&fixture(), name)
}

pub fn validate_toolset(name: &str) -> bool {
    validate_toolset_with_fixture(&fixture(), name)
}

pub fn get_tool_definitions(
    enabled_toolsets: Option<&[String]>,
    disabled_toolsets: Option<&[String]>,
) -> Vec<Value> {
    get_tool_definitions_with_fixture(&fixture(), enabled_toolsets, disabled_toolsets)
}

pub fn dispatch_parity_snapshot() -> DispatchParitySnapshot {
    let mut registry_dispatch = BTreeMap::new();
    for (label, tool_name, mode) in [
        ("success", "typed_tool", HandlerMode::Success),
        ("unknown_tool", "missing_tool", HandlerMode::Unknown),
        (
            "handler_exception",
            "failing_tool",
            HandlerMode::RegistryFailure,
        ),
    ] {
        let result =
            simulated_registry_dispatch(tool_name, &json!({"value": "ok"}), None, None, None, mode)
                .unwrap_or_else(|message| {
                    python_error_string(&format!("Error executing {tool_name}: {message}"))
                });
        registry_dispatch.insert(
            label.to_string(),
            RegistryDispatchCaseSnapshot {
                parsed_result: parse_json_object(&result),
                result,
            },
        );
    }

    let mut handle_function_call = BTreeMap::new();
    for case in dispatch_cases() {
        handle_function_call.insert(case.label.to_string(), simulate_handle_function_call(case));
    }

    DispatchParitySnapshot {
        registry_dispatch,
        handle_function_call,
    }
}

fn all_toolsets_with_fixture(fixture: &RegistryFixture) -> Vec<String> {
    let mut names = fixture.toolsets.keys().cloned().collect::<BTreeSet<_>>();
    names.insert("browser-cdp".to_string());
    names.into_iter().collect()
}

fn validate_toolset_with_fixture(fixture: &RegistryFixture, name: &str) -> bool {
    if matches!(name, "all" | "*") {
        return true;
    }
    name == "browser-cdp"
        || fixture.toolsets.contains_key(name)
        || fixture.legacy_toolsets.contains_key(name)
}

fn resolve_toolset_with_fixture(fixture: &RegistryFixture, name: &str) -> Vec<String> {
    if matches!(name, "all" | "*") {
        let mut all = BTreeSet::new();
        for toolset in fixture.toolsets.keys() {
            all.extend(resolve_toolset_with_fixture(fixture, toolset));
        }
        return all.into_iter().collect();
    }
    if let Some(legacy) = fixture.legacy_toolsets.get(name) {
        return sorted_unique(legacy.iter().cloned());
    }
    if name == "browser-cdp" {
        return vec!["browser_cdp".to_string(), "browser_dialog".to_string()];
    }
    let mut seen = BTreeSet::new();
    resolve_toolset_inner(fixture, name, &mut seen)
}

fn resolve_toolset_inner(
    fixture: &RegistryFixture,
    name: &str,
    seen: &mut BTreeSet<String>,
) -> Vec<String> {
    if !seen.insert(name.to_string()) {
        return Vec::new();
    }
    let Some(toolset) = fixture.toolsets.get(name) else {
        return Vec::new();
    };
    let mut tools: BTreeSet<String> = toolset.tools.iter().cloned().collect();
    for include in &toolset.includes {
        for tool in resolve_toolset_inner(fixture, include, seen) {
            tools.insert(tool);
        }
    }
    tools.into_iter().collect()
}

fn get_tool_definitions_with_fixture(
    fixture: &RegistryFixture,
    enabled_toolsets: Option<&[String]>,
    disabled_toolsets: Option<&[String]>,
) -> Vec<Value> {
    let mut tools_to_include = BTreeSet::new();
    if let Some(enabled) = enabled_toolsets {
        for toolset in enabled {
            if validate_toolset_with_fixture(fixture, toolset) {
                tools_to_include.extend(resolve_toolset_with_fixture(fixture, toolset));
            }
        }
    } else {
        for toolset in fixture.toolsets.keys() {
            tools_to_include.extend(resolve_toolset_with_fixture(fixture, toolset));
        }
    }
    if let Some(disabled) = disabled_toolsets {
        for toolset in disabled {
            if validate_toolset_with_fixture(fixture, toolset) {
                for tool in resolve_toolset_with_fixture(fixture, toolset) {
                    tools_to_include.remove(&tool);
                }
            }
        }
    }

    let schemas_by_name = fixture
        .schemas
        .iter()
        .filter_map(|schema| {
            schema
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(|name| (name.to_string(), schema.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    tools_to_include
        .into_iter()
        .filter_map(|name| schemas_by_name.get(&name).cloned())
        .collect()
}

fn registered_tool_names_with_fixture(fixture: &RegistryFixture) -> Vec<String> {
    schema_names_from_values(&fixture.schemas)
}

fn schema_names_from_values(schemas: &[Value]) -> Vec<String> {
    schemas
        .iter()
        .filter_map(|schema| {
            schema
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect()
}

fn tool_to_toolset_with_fixture(fixture: &RegistryFixture) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (toolset, def) in &fixture.toolsets {
        for tool in &def.tools {
            out.entry(tool.clone()).or_insert_with(|| toolset.clone());
        }
    }
    out
}

fn sorted_unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

const AGENT_LOOP_TOOLS: &[&str] = &["todo", "memory", "session_search", "delegate_task"];
const READ_SEARCH_TOOLS: &[&str] = &["read_file", "search_files"];

#[derive(Clone)]
struct DispatchCase {
    label: &'static str,
    tool_name: &'static str,
    args: Value,
    task_id: Option<&'static str>,
    session_id: Option<&'static str>,
    tool_call_id: Option<&'static str>,
    user_task: Option<&'static str>,
    enabled_tools: Option<Vec<&'static str>>,
    skip_pre_tool_call_hook: bool,
    handler_mode: HandlerMode,
    hooks: HookPlans,
    schema: Option<Value>,
}

#[derive(Clone, Copy)]
enum HandlerMode {
    Success,
    Unknown,
    RegistryFailure,
    OuterException,
}

#[derive(Clone)]
struct HookPlans {
    pre: HookPlan,
    post: HookPlan,
    transform: HookPlan,
}

#[derive(Clone)]
enum HookPlan {
    Return(Vec<Value>),
    Raise,
}

impl HookPlans {
    fn empty() -> Self {
        Self {
            pre: HookPlan::Return(Vec::new()),
            post: HookPlan::Return(Vec::new()),
            transform: HookPlan::Return(Vec::new()),
        }
    }
}

fn dispatch_cases() -> Vec<DispatchCase> {
    let typed_schema = typed_tool_schema();
    vec![
        DispatchCase {
            label: "coerce_success",
            tool_name: "typed_tool",
            args: json!({
                "config": "{\"max\":50}",
                "extra": "42",
                "full": "false",
                "limit": "10",
                "nullable": "null",
                "path": "readme.md",
                "temperature": "0.7",
                "urls": "https://a.com"
            }),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-1"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans::empty(),
            schema: Some(typed_schema.clone()),
        },
        DispatchCase {
            label: "unknown_tool",
            tool_name: "missing_tool",
            args: json!({}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-unknown"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Unknown,
            hooks: HookPlans::empty(),
            schema: None,
        },
        DispatchCase {
            label: "handler_exception",
            tool_name: "failing_tool",
            args: json!({"value": "x"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-fail"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::RegistryFailure,
            hooks: HookPlans::empty(),
            schema: None,
        },
        DispatchCase {
            label: "outer_dispatch_exception",
            tool_name: "exploding_dispatch",
            args: json!({"value": "x"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-outer"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::OuterException,
            hooks: HookPlans::empty(),
            schema: None,
        },
        DispatchCase {
            label: "agent_loop_tool",
            tool_name: "todo",
            args: json!({"action": "list"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-agent-loop"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans::empty(),
            schema: None,
        },
        DispatchCase {
            label: "pre_hook_block",
            tool_name: "web_search",
            args: json!({"q": "test"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-block"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(vec![
                    json!({"action": "block", "message": "Blocked by policy"}),
                ]),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Return(Vec::new()),
            },
            schema: None,
        },
        DispatchCase {
            label: "invalid_pre_hook_returns",
            tool_name: "web_search",
            args: json!({"q": "test"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-invalid-pre"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(vec![
                    json!("block"),
                    json!({"action": "block"}),
                    json!({"action": "deny", "message": "nope"}),
                ]),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Return(Vec::new()),
            },
            schema: None,
        },
        DispatchCase {
            label: "skip_pre_hook",
            tool_name: "web_search",
            args: json!({"q": "test"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-skip"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: true,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(vec![
                    json!({"action": "block", "message": "should not fire"}),
                ]),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Return(Vec::new()),
            },
            schema: None,
        },
        DispatchCase {
            label: "post_observational",
            tool_name: "typed_tool",
            args: json!({"limit": "3"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-post"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: true,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(Vec::new()),
                post: HookPlan::Return(vec![json!("observer return should be ignored")]),
                transform: HookPlan::Return(Vec::new()),
            },
            schema: Some(typed_schema.clone()),
        },
        DispatchCase {
            label: "transform_first_string",
            tool_name: "typed_tool",
            args: json!({"limit": "4"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-transform"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: true,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(Vec::new()),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Return(vec![
                    Value::Null,
                    json!({"bad": true}),
                    json!("rewritten"),
                    json!("second"),
                ]),
            },
            schema: Some(typed_schema.clone()),
        },
        DispatchCase {
            label: "non_string_transform_ignored",
            tool_name: "typed_tool",
            args: json!({"limit": "5"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-transform-ignore"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: true,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(Vec::new()),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Return(vec![
                    json!({"bad": true}),
                    json!(123),
                    json!(["nope"]),
                ]),
            },
            schema: Some(typed_schema.clone()),
        },
        DispatchCase {
            label: "transform_hook_exception",
            tool_name: "typed_tool",
            args: json!({"limit": "6"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-transform-exception"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: true,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans {
                pre: HookPlan::Return(Vec::new()),
                post: HookPlan::Return(Vec::new()),
                transform: HookPlan::Raise,
            },
            schema: Some(typed_schema.clone()),
        },
        DispatchCase {
            label: "execute_code_enabled_tools",
            tool_name: "execute_code",
            args: json!({"code": "print(1)"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-code"),
            user_task: Some("user goal"),
            enabled_tools: Some(vec!["terminal", "read_file"]),
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans::empty(),
            schema: None,
        },
        DispatchCase {
            label: "read_file_no_notify",
            tool_name: "read_file",
            args: json!({"limit": "100", "offset": "10", "path": "foo.py"}),
            task_id: Some("task-1"),
            session_id: Some("session-1"),
            tool_call_id: Some("call-read"),
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans::empty(),
            schema: Some(read_file_schema()),
        },
        DispatchCase {
            label: "terminal_default_task_notification",
            tool_name: "terminal",
            args: json!({"command": "pwd"}),
            task_id: None,
            session_id: None,
            tool_call_id: None,
            user_task: Some("user goal"),
            enabled_tools: None,
            skip_pre_tool_call_hook: false,
            handler_mode: HandlerMode::Success,
            hooks: HookPlans::empty(),
            schema: None,
        },
    ]
}

fn simulate_handle_function_call(case: DispatchCase) -> FunctionCallCaseSnapshot {
    let args = coerce_tool_args_value(&case.args, case.schema.as_ref());
    let mut state = DispatchState::default();

    let result = match simulate_handle_function_call_inner(&case, &args, &mut state) {
        Ok(result) => result,
        Err(message) => {
            python_error_string(&format!("Error executing {}: {message}", case.tool_name))
        }
    };

    FunctionCallCaseSnapshot {
        args_after_coercion: args,
        dispatches: state.dispatches,
        hook_events: state.hook_events,
        notifications: state.notifications,
        parsed_result: parse_json_object(&result),
        result,
    }
}

fn simulate_handle_function_call_inner(
    case: &DispatchCase,
    args: &Value,
    state: &mut DispatchState,
) -> Result<String, String> {
    if AGENT_LOOP_TOOLS.contains(&case.tool_name) {
        return Ok(python_error_string(&format!(
            "{} must be handled by the agent loop",
            case.tool_name
        )));
    }

    if !case.skip_pre_tool_call_hook {
        if let Ok(hook_results) = invoke_simulated_hook("pre_tool_call", case, args, None, state) {
            if let Some(block_message) = first_block_message(&hook_results) {
                return Ok(python_error_string(&block_message));
            }
        }
    }

    if !READ_SEARCH_TOOLS.contains(&case.tool_name) {
        state
            .notifications
            .push(case.task_id.unwrap_or("default").to_string());
    }

    let result = if case.tool_name == "execute_code" {
        let enabled_tools = case.enabled_tools.clone().map(|items| {
            items
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        });
        state.dispatches.push(DispatchInvocation {
            tool_name: case.tool_name.to_string(),
            args: args.clone(),
            task_id: case.task_id.map(ToString::to_string),
            user_task: None,
            enabled_tools: enabled_tools.clone(),
        });
        simulated_registry_dispatch(
            case.tool_name,
            args,
            case.task_id,
            None,
            case.enabled_tools.clone(),
            case.handler_mode,
        )?
    } else {
        state.dispatches.push(DispatchInvocation {
            tool_name: case.tool_name.to_string(),
            args: args.clone(),
            task_id: case.task_id.map(ToString::to_string),
            user_task: case.user_task.map(ToString::to_string),
            enabled_tools: None,
        });
        simulated_registry_dispatch(
            case.tool_name,
            args,
            case.task_id,
            case.user_task,
            None,
            case.handler_mode,
        )?
    };

    let _ = invoke_simulated_hook("post_tool_call", case, args, Some(&result), state);
    let mut transformed = result;
    if let Ok(hook_results) = invoke_simulated_hook(
        "transform_tool_result",
        case,
        args,
        Some(&transformed),
        state,
    ) {
        for hook_result in hook_results {
            if let Some(s) = hook_result.as_str() {
                transformed = s.to_string();
                break;
            }
        }
    }

    Ok(transformed)
}

fn simulated_registry_dispatch(
    tool_name: &str,
    args: &Value,
    task_id: Option<&str>,
    user_task: Option<&str>,
    enabled_tools: Option<Vec<&str>>,
    mode: HandlerMode,
) -> Result<String, String> {
    match mode {
        HandlerMode::Unknown => Ok(python_error_string(&format!("Unknown tool: {tool_name}"))),
        HandlerMode::RegistryFailure => Ok(python_error_string(
            "Tool execution failed: RuntimeError: boom",
        )),
        HandlerMode::OuterException => Err("outer boom".to_string()),
        HandlerMode::Success => {
            let enabled_tools_value = enabled_tools.clone().map(|items| {
                items
                    .into_iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            });
            let payload = json!({
                "args": args,
                "enabled_tools": enabled_tools_value,
                "task_id": task_id,
                "tool_name": tool_name,
                "user_task": user_task,
            });
            Ok(serde_json::to_string(&payload).expect("handler payload serializes"))
        }
    }
}

#[derive(Default)]
struct DispatchState {
    dispatches: Vec<DispatchInvocation>,
    hook_events: Vec<HookObservation>,
    notifications: Vec<String>,
}

fn invoke_simulated_hook(
    hook_name: &str,
    case: &DispatchCase,
    args: &Value,
    result: Option<&String>,
    state: &mut DispatchState,
) -> Result<Vec<Value>, String> {
    let plan = match hook_name {
        "pre_tool_call" => &case.hooks.pre,
        "post_tool_call" => &case.hooks.post,
        "transform_tool_result" => &case.hooks.transform,
        _ => return Ok(Vec::new()),
    };

    state.hook_events.push(HookObservation {
        hook: hook_name.to_string(),
        tool_name: case.tool_name.to_string(),
        args: if args.is_object() {
            args.clone()
        } else {
            json!({})
        },
        result: result.cloned(),
        task_id: case.task_id.unwrap_or("").to_string(),
        session_id: case.session_id.unwrap_or("").to_string(),
        tool_call_id: case.tool_call_id.unwrap_or("").to_string(),
        duration_ms: if matches!(hook_name, "post_tool_call" | "transform_tool_result") {
            Some("non_negative_int".to_string())
        } else {
            None
        },
    });

    match plan {
        HookPlan::Return(values) => Ok(values.clone()),
        HookPlan::Raise => Err("hook boom".to_string()),
    }
}

fn first_block_message(results: &[Value]) -> Option<String> {
    for result in results {
        let Some(obj) = result.as_object() else {
            continue;
        };
        if obj.get("action").and_then(Value::as_str) != Some("block") {
            continue;
        }
        if let Some(message) = obj.get("message").and_then(Value::as_str) {
            if !message.is_empty() {
                return Some(message.to_string());
            }
        }
    }
    None
}

fn coerce_tool_args_value(args: &Value, schema: Option<&Value>) -> Value {
    let Some(args_object) = args.as_object() else {
        return args.clone();
    };
    let Some(properties) = schema
        .and_then(|s| s.get("parameters"))
        .and_then(|p| p.get("properties"))
        .and_then(Value::as_object)
    else {
        return args.clone();
    };

    let mut out = args_object.clone();
    let keys = out.keys().cloned().collect::<Vec<_>>();
    for key in keys {
        let Some(prop_schema) = properties.get(&key) else {
            continue;
        };
        let Some(value) = out.get(&key).cloned() else {
            continue;
        };
        let expected = prop_schema.get("type");

        if expected == Some(&Value::String("array".to_string()))
            && !value.is_null()
            && !value.is_array()
        {
            if let Some(s) = value.as_str() {
                if let Some(coerced) = coerce_value(s, expected, Some(prop_schema)) {
                    out.insert(key, coerced);
                } else {
                    out.insert(key, json!([s]));
                }
            } else {
                out.insert(key, Value::Array(vec![value]));
            }
            continue;
        }

        let Some(s) = value.as_str() else {
            continue;
        };
        if expected.is_none() && !schema_allows_null(Some(prop_schema)) {
            continue;
        }
        if let Some(coerced) = coerce_value(s, expected, Some(prop_schema)) {
            out.insert(key, coerced);
        }
    }

    Value::Object(out)
}

fn coerce_value(
    value: &str,
    expected_type: Option<&Value>,
    schema: Option<&Value>,
) -> Option<Value> {
    if schema_allows_null(schema) && value.trim().eq_ignore_ascii_case("null") {
        return Some(Value::Null);
    }

    match expected_type {
        Some(Value::Array(types)) => {
            for ty in types {
                if let Some(result) = coerce_value(value, Some(ty), schema) {
                    return Some(result);
                }
            }
            None
        }
        Some(Value::String(kind)) if kind == "integer" => coerce_number(value, true),
        Some(Value::String(kind)) if kind == "number" => coerce_number(value, false),
        Some(Value::String(kind)) if kind == "boolean" => coerce_boolean(value),
        Some(Value::String(kind)) if kind == "array" => coerce_json(value, true),
        Some(Value::String(kind)) if kind == "object" => coerce_json(value, false),
        Some(Value::String(kind))
            if kind == "null" && value.trim().eq_ignore_ascii_case("null") =>
        {
            Some(Value::Null)
        }
        _ => None,
    }
}

fn schema_allows_null(schema: Option<&Value>) -> bool {
    let Some(schema) = schema.and_then(Value::as_object) else {
        return false;
    };

    match schema.get("type") {
        Some(Value::String(kind)) if kind == "null" => return true,
        Some(Value::Array(types)) if types.iter().any(|ty| ty.as_str() == Some("null")) => {
            return true;
        }
        _ => {}
    }
    if schema.get("nullable").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    for union_key in ["anyOf", "oneOf"] {
        let Some(variants) = schema.get(union_key).and_then(Value::as_array) else {
            continue;
        };
        if variants
            .iter()
            .any(|variant| variant.get("type").and_then(Value::as_str) == Some("null"))
        {
            return true;
        }
    }
    false
}

fn coerce_number(value: &str, integer_only: bool) -> Option<Value> {
    let parsed = value.parse::<f64>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    if parsed.fract() == 0.0 {
        return Some(Value::Number(Number::from(parsed as i64)));
    }
    if integer_only {
        return None;
    }
    Number::from_f64(parsed).map(Value::Number)
}

fn coerce_boolean(value: &str) -> Option<Value> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(Value::Bool(true)),
        "false" => Some(Value::Bool(false)),
        _ => None,
    }
}

fn coerce_json(value: &str, expect_array: bool) -> Option<Value> {
    let parsed: Value = serde_json::from_str(value).ok()?;
    if (expect_array && parsed.is_array()) || (!expect_array && parsed.is_object()) {
        Some(parsed)
    } else {
        None
    }
}

fn typed_tool_schema() -> Value {
    json!({
        "name": "typed_tool",
        "description": "test",
        "parameters": {
            "type": "object",
            "properties": {
                "config": {"type": "object"},
                "full": {"type": "boolean"},
                "limit": {"type": "integer"},
                "nullable": {"type": "object", "nullable": true, "default": null},
                "path": {"type": "string"},
                "temperature": {"type": "number"},
                "urls": {"type": "array", "items": {"type": "string"}}
            }
        }
    })
}

fn read_file_schema() -> Value {
    json!({
        "name": "read_file",
        "description": "test",
        "parameters": {
            "type": "object",
            "properties": {
                "limit": {"type": "integer"},
                "offset": {"type": "integer"},
                "path": {"type": "string"}
            }
        }
    })
}

fn python_error_string(message: &str) -> String {
    format!(r#"{{"error": "{message}"}}"#)
}

fn parse_json_object(result: &str) -> Option<Value> {
    serde_json::from_str(result).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_composed_and_legacy_toolsets() {
        assert!(resolve_toolset("debugging").contains(&"terminal".to_string()));
        assert!(resolve_toolset("debugging").contains(&"read_file".to_string()));
        assert_eq!(
            resolve_toolset("file_tools"),
            vec!["patch", "read_file", "search_files", "write_file"]
        );
    }

    #[test]
    fn disabled_toolsets_are_subtracted_after_enable() {
        let enabled = vec!["hermes-cli".to_string()];
        let disabled = vec!["browser".to_string()];
        let schemas = get_tool_definitions(Some(&enabled), Some(&disabled));
        let names = schema_names_from_values(&schemas);
        assert!(!names.iter().any(|name| name.starts_with("browser_")));
        assert!(names.contains(&"terminal".to_string()));
    }

    #[test]
    fn cache_isolation_smoke_has_distinct_results() {
        let snapshot = tool_registry_snapshot();
        assert_eq!(
            snapshot.cache_isolation.default_names_before,
            snapshot.cache_isolation.default_names_after
        );
        assert!(snapshot.cache_isolation.cache_keys_distinct);
    }

    #[test]
    fn dispatch_snapshot_covers_hook_and_error_edges() {
        let snapshot = dispatch_parity_snapshot();
        assert_eq!(
            snapshot.registry_dispatch["unknown_tool"].result,
            r#"{"error": "Unknown tool: missing_tool"}"#
        );
        assert_eq!(
            snapshot.handle_function_call["agent_loop_tool"].result,
            r#"{"error": "todo must be handled by the agent loop"}"#
        );
        assert!(snapshot.handle_function_call["read_file_no_notify"]
            .notifications
            .is_empty());
        assert_eq!(
            snapshot.handle_function_call["transform_first_string"].result,
            "rewritten"
        );
        assert_eq!(
            snapshot.handle_function_call["execute_code_enabled_tools"].dispatches[0].enabled_tools,
            Some(vec!["terminal".to_string(), "read_file".to_string()])
        );
    }
}

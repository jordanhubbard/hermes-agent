use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcpServerMethod {
    pub name: String,
    pub response: String,
    pub session_effects: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvertisedCommand {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilitySnapshot {
    pub load_session: bool,
    pub prompt_image: bool,
    pub session_fork: bool,
    pub session_list: bool,
    pub session_resume: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionContract {
    pub state_fields: Vec<String>,
    pub manager_methods: Vec<String>,
    pub source: String,
    pub model_config_keys: Vec<String>,
    pub list_page_size: usize,
    pub expanded_toolsets: Vec<String>,
    pub cwd_normalization_samples: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermissionCase {
    pub option_id: String,
    pub kind: String,
    pub hermes_result: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EventCallbackContract {
    pub callback: String,
    pub input: String,
    pub update: String,
    pub tracking: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolTitleSample {
    pub tool: String,
    pub args: Value,
    pub kind: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolRenderingContract {
    pub tool: String,
    pub phase: String,
    pub kind: String,
    pub content: String,
    pub raw_passthrough: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeBoundary {
    pub surface: String,
    pub boundary: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AcpParitySnapshot {
    pub protocol: String,
    pub capabilities: CapabilitySnapshot,
    pub server_methods: Vec<AcpServerMethod>,
    pub advertised_commands: Vec<AdvertisedCommand>,
    pub session: SessionContract,
    pub permission_cases: Vec<PermissionCase>,
    pub tool_kind_map: BTreeMap<String, String>,
    pub polished_tools: Vec<String>,
    pub tool_title_samples: Vec<ToolTitleSample>,
    pub tool_rendering: Vec<ToolRenderingContract>,
    pub event_callbacks: Vec<EventCallbackContract>,
    pub runtime_boundaries: Vec<RuntimeBoundary>,
}

pub fn acp_parity_snapshot() -> AcpParitySnapshot {
    AcpParitySnapshot {
        protocol: "agent-client-protocol.hermes.v1".to_string(),
        capabilities: CapabilitySnapshot {
            load_session: true,
            prompt_image: true,
            session_fork: true,
            session_list: true,
            session_resume: true,
        },
        server_methods: server_methods(),
        advertised_commands: advertised_commands(),
        session: session_contract(),
        permission_cases: permission_cases(),
        tool_kind_map: tool_kind_map(),
        polished_tools: polished_tools(),
        tool_title_samples: tool_title_samples(),
        tool_rendering: tool_rendering_contracts(),
        event_callbacks: event_callbacks(),
        runtime_boundaries: vec![
            RuntimeBoundary {
                surface: "ACP protocol".to_string(),
                boundary: "rust_contract_python_runtime".to_string(),
                reason: "Rust captures ACP capabilities, server method shapes, session persistence fields, permission outcomes, event callback routing, and tool rendering contracts. Live stdio serving and AIAgent execution remain Python-bound until the runtime cutover.".to_string(),
            },
            RuntimeBoundary {
                surface: "SessionDB".to_string(),
                boundary: "rust_state_backend".to_string(),
                reason: "ACP sessions persist through the shared SessionDB factory, so this contract keeps source='acp', model_config cwd/provider metadata, and conversation history replay aligned with the Rust state backend already gated elsewhere.".to_string(),
            },
        ],
    }
}

pub fn get_tool_kind(tool_name: &str) -> String {
    tool_kind_map()
        .get(tool_name)
        .cloned()
        .unwrap_or_else(|| "other".to_string())
}

pub fn build_tool_title(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "terminal" => {
            let command = truncate_suffix(value_str(args, "command"), 80, 77);
            format!("terminal: {command}")
        }
        "read_file" => format!("read: {}", value_or(args, "path", "?")),
        "write_file" => format!("write: {}", value_or(args, "path", "?")),
        "patch" => format!(
            "patch ({}): {}",
            value_or(args, "mode", "replace"),
            value_or(args, "path", "?")
        ),
        "search_files" => format!("search: {}", value_or(args, "pattern", "?")),
        "web_search" => format!("web search: {}", value_or(args, "query", "?")),
        "web_extract" => {
            let urls = args.get("urls").and_then(Value::as_array);
            match urls.and_then(|items| items.first()).and_then(Value::as_str) {
                Some(first) => {
                    let extra = urls.map(|items| items.len().saturating_sub(1)).unwrap_or(0);
                    if extra > 0 {
                        format!("extract: {first} (+{extra})")
                    } else {
                        format!("extract: {first}")
                    }
                }
                None => "web extract".to_string(),
            }
        }
        "process" => {
            let action = nonempty_or(value_str(args, "action"), "manage");
            let sid = value_str(args, "session_id");
            if sid.trim().is_empty() {
                format!("process {action}")
            } else {
                format!("process {action}: {}", sid.trim())
            }
        }
        "delegate_task" => {
            if let Some(tasks) = args.get("tasks").and_then(Value::as_array) {
                if !tasks.is_empty() {
                    return format!("delegate batch ({} tasks)", tasks.len());
                }
            }
            let goal = truncate_suffix(value_str(args, "goal"), 60, 57);
            if goal.trim().is_empty() {
                "delegate task".to_string()
            } else {
                format!("delegate: {goal}")
            }
        }
        "session_search" => {
            let query = value_str(args, "query");
            if query.trim().is_empty() {
                "recent sessions".to_string()
            } else {
                format!("session search: {}", query.trim())
            }
        }
        "memory" => format!(
            "memory {}: {}",
            nonempty_or(value_str(args, "action"), "manage"),
            nonempty_or(value_str(args, "target"), "memory")
        ),
        "execute_code" => {
            let first_line = value_str(args, "code")
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("")
                .to_string();
            let first_line = truncate_suffix(first_line, 70, 67);
            if first_line.is_empty() {
                "python code".to_string()
            } else {
                format!("python: {first_line}")
            }
        }
        "todo" => {
            if let Some(items) = args.get("todos").and_then(Value::as_array) {
                format!(
                    "todo ({} item{})",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" }
                )
            } else {
                "todo".to_string()
            }
        }
        "skill_view" => {
            let name = nonempty_or(value_str(args, "name"), "?");
            let file_path = value_str(args, "file_path");
            if file_path.trim().is_empty() {
                format!("skill view ({name})")
            } else {
                format!("skill view ({name}/{})", file_path.trim())
            }
        }
        "skills_list" => {
            let category = value_str(args, "category");
            if category.trim().is_empty() {
                "skills list".to_string()
            } else {
                format!("skills list ({})", category.trim())
            }
        }
        "skill_manage" => {
            let action = nonempty_or(value_str(args, "action"), "manage");
            let name = nonempty_or(value_str(args, "name"), "?");
            let file_path = value_str(args, "file_path");
            let target = if file_path.trim().is_empty() {
                name
            } else {
                format!("{name}/{}", file_path.trim())
            };
            format!("skill {action}: {}", truncate_suffix(target, 64, 61))
        }
        "browser_navigate" => format!("navigate: {}", value_or(args, "url", "?")),
        "browser_snapshot" => "browser snapshot".to_string(),
        "browser_vision" => format!(
            "browser vision: {}",
            truncate_prefix(value_or(args, "question", "?"), 50)
        ),
        "browser_get_images" => "browser images".to_string(),
        "vision_analyze" => format!(
            "analyze image: {}",
            truncate_prefix(value_or(args, "question", "?"), 50)
        ),
        "image_generate" => {
            let prompt = value_str(args, "prompt");
            let prompt = if prompt.trim().is_empty() {
                value_str(args, "description")
            } else {
                prompt
            };
            if prompt.trim().is_empty() {
                "generate image".to_string()
            } else {
                format!(
                    "generate image: {}",
                    truncate_prefix(prompt.trim().to_string(), 50)
                )
            }
        }
        "cronjob" => {
            let action = nonempty_or(value_str(args, "action"), "manage");
            let job_id = value_str(args, "job_id");
            let id = if job_id.trim().is_empty() {
                value_str(args, "id")
            } else {
                job_id
            };
            if id.trim().is_empty() {
                format!("cron {action}")
            } else {
                format!("cron {action}: {}", id.trim())
            }
        }
        _ => tool_name.to_string(),
    }
}

pub fn expand_acp_enabled_toolsets(toolsets: &[&str], mcp_server_names: &[&str]) -> Vec<String> {
    let mut expanded = Vec::new();
    let base: Vec<&str> = if toolsets.is_empty() {
        vec!["hermes-acp"]
    } else {
        toolsets.to_vec()
    };
    for name in base {
        if !name.is_empty() && !expanded.iter().any(|item| item == name) {
            expanded.push(name.to_string());
        }
    }
    for server_name in mcp_server_names {
        let toolset = format!("mcp-{server_name}");
        if !server_name.is_empty() && !expanded.iter().any(|item| item == &toolset) {
            expanded.push(toolset);
        }
    }
    expanded
}

pub fn normalize_cwd_for_compare(cwd: &str) -> String {
    let raw = if cwd.trim().is_empty() {
        "."
    } else {
        cwd.trim()
    };
    if let Some(converted) = win_path_to_wsl(raw) {
        return normalize_slashes(&converted);
    }
    if raw.len() > 7 && raw[..5].eq_ignore_ascii_case("/mnt/") {
        let mut chars: Vec<char> = raw.chars().collect();
        if chars.get(5) == Some(&'/') {
            return normalize_slashes(raw);
        }
        if let Some(drive) = chars.get_mut(5) {
            *drive = drive.to_ascii_lowercase();
        }
        return normalize_slashes(&chars.into_iter().collect::<String>());
    }
    normalize_slashes(raw)
}

pub fn permission_result_for_kind(kind: &str) -> &'static str {
    match kind {
        "allow_once" => "once",
        "allow_always" => "always",
        "reject_once" | "reject_always" => "deny",
        _ => "deny",
    }
}

fn server_methods() -> Vec<AcpServerMethod> {
    vec![
        method(
            "initialize",
            "InitializeResponse",
            &["advertise_capabilities", "auth_methods"],
        ),
        method(
            "authenticate",
            "AuthenticateResponse|None",
            &["provider_match_required"],
        ),
        method(
            "new_session",
            "NewSessionResponse",
            &[
                "create_session",
                "register_mcp",
                "available_commands_update",
                "usage_update",
            ],
        ),
        method(
            "load_session",
            "LoadSessionResponse|None",
            &[
                "update_cwd",
                "register_mcp",
                "history_replay",
                "available_commands_update",
                "usage_update",
            ],
        ),
        method(
            "resume_session",
            "ResumeSessionResponse",
            &[
                "update_or_create_session",
                "register_mcp",
                "history_replay",
                "available_commands_update",
                "usage_update",
            ],
        ),
        method(
            "cancel",
            "None",
            &[
                "set_cancel_event",
                "agent_interrupt",
                "preserve_interrupted_prompt",
            ],
        ),
        method(
            "fork_session",
            "ForkSessionResponse",
            &["fork_history", "register_mcp", "available_commands_update"],
        ),
        method(
            "list_sessions",
            "ListSessionsResponse",
            &["cwd_filter", "cursor_pagination"],
        ),
        method(
            "prompt",
            "PromptResponse",
            &[
                "stream_agent_message",
                "tool_callbacks",
                "approval_callback",
                "persist_history",
                "drain_queue",
                "usage",
            ],
        ),
        method(
            "set_session_model",
            "SetSessionModelResponse|None",
            &["resolve_provider_model", "recreate_agent", "persist"],
        ),
        method(
            "set_session_mode",
            "SetSessionModeResponse|None",
            &["persist_mode"],
        ),
        method(
            "set_config_option",
            "SetSessionConfigOptionResponse|None",
            &["persist_config_option"],
        ),
    ]
}

fn method(name: &str, response: &str, session_effects: &[&str]) -> AcpServerMethod {
    AcpServerMethod {
        name: name.to_string(),
        response: response.to_string(),
        session_effects: strings(session_effects),
    }
}

fn advertised_commands() -> Vec<AdvertisedCommand> {
    vec![
        command("help", "List available commands", None),
        command(
            "model",
            "Show current model and provider, or switch models",
            Some("model name to switch to"),
        ),
        command("tools", "List available tools with descriptions", None),
        command("context", "Show conversation message counts by role", None),
        command("reset", "Clear conversation history", None),
        command("compact", "Compress conversation context", None),
        command(
            "steer",
            "Inject guidance into the currently running agent turn",
            Some("guidance for the active turn"),
        ),
        command(
            "queue",
            "Queue a prompt to run after the current turn finishes",
            Some("prompt to run next"),
        ),
        command("version", "Show Hermes version", None),
    ]
}

fn command(name: &str, description: &str, input_hint: Option<&str>) -> AdvertisedCommand {
    AdvertisedCommand {
        name: name.to_string(),
        description: description.to_string(),
        input_hint: input_hint.map(str::to_string),
    }
}

fn session_contract() -> SessionContract {
    let mut samples = BTreeMap::new();
    samples.insert(
        "E:\\Projects\\AI\\paperclip".to_string(),
        "/mnt/e/Projects/AI/paperclip".to_string(),
    );
    samples.insert(
        "D:/work/project".to_string(),
        "/mnt/d/work/project".to_string(),
    );
    samples.insert(
        "/mnt/E/Projects/AI/paperclip".to_string(),
        "/mnt/e/Projects/AI/paperclip".to_string(),
    );

    SessionContract {
        state_fields: strings(&[
            "session_id",
            "agent",
            "cwd",
            "model",
            "history",
            "cancel_event",
            "is_running",
            "queued_prompts",
            "runtime_lock",
            "current_prompt_text",
            "interrupted_prompt_text",
        ]),
        manager_methods: strings(&[
            "create_session",
            "get_session",
            "remove_session",
            "fork_session",
            "list_sessions",
            "update_cwd",
            "cleanup",
            "save_session",
        ]),
        source: "acp".to_string(),
        model_config_keys: strings(&["cwd", "provider", "base_url", "api_mode"]),
        list_page_size: 50,
        expanded_toolsets: expand_acp_enabled_toolsets(&["hermes-acp"], &["filesystem", "github"]),
        cwd_normalization_samples: samples,
    }
}

fn permission_cases() -> Vec<PermissionCase> {
    vec![
        permission("allow_once", "allow_once"),
        permission("allow_always", "allow_always"),
        permission("deny", "reject_once"),
        permission("deny_always", "reject_always"),
        permission("timeout", "timeout"),
        permission("none_response", "none"),
    ]
}

fn permission(option_id: &str, kind: &str) -> PermissionCase {
    let hermes_result = match kind {
        "timeout" | "none" => "deny",
        _ => permission_result_for_kind(kind),
    };
    PermissionCase {
        option_id: option_id.to_string(),
        kind: kind.to_string(),
        hermes_result: hermes_result.to_string(),
    }
}

fn event_callbacks() -> Vec<EventCallbackContract> {
    vec![
        callback(
            "tool_progress_callback",
            "tool.started",
            "ToolCallStart",
            &[
                "parse_string_args",
                "coerce_non_dict_args",
                "fifo_id_by_tool_name",
                "capture_edit_snapshot",
            ],
        ),
        callback(
            "step_callback",
            "prev_tools",
            "ToolCallProgress",
            &[
                "pop_fifo_id_by_tool_name",
                "forward_result",
                "drop_empty_queue",
            ],
        ),
        callback(
            "thinking_callback",
            "reasoning text",
            "AgentThoughtChunk",
            &["skip_empty_text"],
        ),
        callback(
            "message_callback",
            "stream delta",
            "AgentMessageChunk",
            &["skip_empty_text"],
        ),
        callback(
            "usage_update",
            "context pressure",
            "UsageUpdate",
            &["size", "used"],
        ),
        callback(
            "available_commands",
            "session start/load/resume",
            "AvailableCommandsUpdate",
            &["advertised_commands"],
        ),
    ]
}

fn callback(callback: &str, input: &str, update: &str, tracking: &[&str]) -> EventCallbackContract {
    EventCallbackContract {
        callback: callback.to_string(),
        input: input.to_string(),
        update: update.to_string(),
        tracking: strings(tracking),
    }
}

fn tool_title_samples() -> Vec<ToolTitleSample> {
    let samples = vec![
        ("terminal", json!({"command": "ls -la /tmp"})),
        ("terminal", json!({"command": "x".repeat(200)})),
        ("read_file", json!({"path": "/etc/hosts"})),
        ("patch", json!({"path": "main.py", "mode": "replace"})),
        ("search_files", json!({"pattern": "TODO"})),
        ("web_search", json!({"query": "python asyncio"})),
        (
            "web_extract",
            json!({"urls": ["https://example.com/docs", "https://example.com/2"]}),
        ),
        ("skill_view", json!({"name": "github-pitfalls"})),
        (
            "skill_view",
            json!({"name": "github-pitfalls", "file_path": "references/api.md"}),
        ),
        (
            "execute_code",
            json!({"code": "\nfrom hermes_tools import terminal\nprint('done')"}),
        ),
        (
            "skill_manage",
            json!({"action": "patch", "name": "hermes-agent-operations", "file_path": "references/acp.md"}),
        ),
        ("delegate_task", json!({"tasks": [{"goal": "Review ACP"}]})),
        ("session_search", json!({"query": "zed"})),
        ("memory", json!({"action": "add", "target": "user"})),
        ("todo", json!({"todos": [{"id": "one"}]})),
        ("browser_navigate", json!({"url": "https://example.com"})),
        ("image_generate", json!({"prompt": "diagram"})),
        ("cronjob", json!({"action": "run", "job_id": "daily"})),
        ("some_new_tool", json!({"foo": "bar"})),
    ];
    samples
        .into_iter()
        .map(|(tool, args)| ToolTitleSample {
            tool: tool.to_string(),
            kind: get_tool_kind(tool),
            title: build_tool_title(tool, &args),
            args,
        })
        .collect()
}

fn tool_rendering_contracts() -> Vec<ToolRenderingContract> {
    vec![
        render("read_file", "start", "read", "compact_no_content", false),
        render("web_extract", "start", "fetch", "compact_no_content", false),
        render("patch", "start", "edit", "structured_diff", false),
        render("write_file", "start", "edit", "structured_diff", false),
        render("terminal", "start", "execute", "text_command", false),
        render("search_files", "start", "search", "text_pattern", false),
        render("todo", "start", "other", "human_readable_list", false),
        render(
            "execute_code",
            "start",
            "execute",
            "fenced_python_preview",
            false,
        ),
        render(
            "patch",
            "complete",
            "edit",
            "structured_diff_or_text_fallback",
            false,
        ),
        render(
            "write_file",
            "complete",
            "edit",
            "snapshot_diff_or_text_fallback",
            false,
        ),
        render(
            "web_extract",
            "complete",
            "fetch",
            "compact_on_success_error_text_on_failure",
            false,
        ),
        render("terminal", "complete", "execute", "polished_text", false),
        render("unknown", "complete", "other", "generic_text", true),
    ]
}

fn render(
    tool: &str,
    phase: &str,
    kind: &str,
    content: &str,
    raw_passthrough: bool,
) -> ToolRenderingContract {
    ToolRenderingContract {
        tool: tool.to_string(),
        phase: phase.to_string(),
        kind: kind.to_string(),
        content: content.to_string(),
        raw_passthrough,
    }
}

fn tool_kind_map() -> BTreeMap<String, String> {
    let pairs = [
        ("read_file", "read"),
        ("write_file", "edit"),
        ("patch", "edit"),
        ("search_files", "search"),
        ("terminal", "execute"),
        ("process", "execute"),
        ("execute_code", "execute"),
        ("todo", "other"),
        ("skill_view", "read"),
        ("skills_list", "read"),
        ("skill_manage", "edit"),
        ("web_search", "fetch"),
        ("web_extract", "fetch"),
        ("browser_navigate", "fetch"),
        ("browser_click", "execute"),
        ("browser_type", "execute"),
        ("browser_snapshot", "read"),
        ("browser_vision", "read"),
        ("browser_scroll", "execute"),
        ("browser_press", "execute"),
        ("browser_back", "execute"),
        ("browser_get_images", "read"),
        ("delegate_task", "execute"),
        ("vision_analyze", "read"),
        ("image_generate", "execute"),
        ("text_to_speech", "execute"),
        ("_thinking", "think"),
    ];
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn polished_tools() -> Vec<String> {
    strings(&[
        "todo",
        "memory",
        "session_search",
        "delegate_task",
        "read_file",
        "write_file",
        "patch",
        "search_files",
        "terminal",
        "process",
        "execute_code",
        "skill_view",
        "skills_list",
        "skill_manage",
        "web_search",
        "web_extract",
        "browser_navigate",
        "browser_click",
        "browser_type",
        "browser_press",
        "browser_scroll",
        "browser_back",
        "browser_snapshot",
        "browser_console",
        "browser_get_images",
        "browser_vision",
        "vision_analyze",
        "image_generate",
        "text_to_speech",
        "cronjob",
        "send_message",
        "clarify",
        "discord",
        "discord_admin",
        "ha_list_entities",
        "ha_get_state",
        "ha_list_services",
        "ha_call_service",
        "feishu_doc_read",
        "feishu_drive_list_comments",
        "feishu_drive_list_comment_replies",
        "feishu_drive_reply_comment",
        "feishu_drive_add_comment",
        "kanban_create",
        "kanban_show",
        "kanban_comment",
        "kanban_complete",
        "kanban_block",
        "kanban_link",
        "kanban_heartbeat",
        "yb_query_group_info",
        "yb_query_group_members",
        "yb_search_sticker",
        "yb_send_dm",
        "yb_send_sticker",
        "mixture_of_agents",
    ])
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn value_str(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn value_or(args: &Value, key: &str, default: &str) -> String {
    let value = value_str(args, key);
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

fn nonempty_or(value: String, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn truncate_suffix(value: String, max_len: usize, keep: usize) -> String {
    if value.chars().count() > max_len {
        let prefix: String = value.chars().take(keep).collect();
        format!("{prefix}...")
    } else {
        value
    }
}

fn truncate_prefix(value: String, max_len: usize) -> String {
    value.chars().take(max_len).collect()
}

fn win_path_to_wsl(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    let drive = bytes[0] as char;
    if !drive.is_ascii_alphabetic() || bytes[1] != b':' || (bytes[2] != b'\\' && bytes[2] != b'/') {
        return None;
    }
    let tail = path[3..].replace('\\', "/");
    Some(format!("/mnt/{}/{}", drive.to_ascii_lowercase(), tail))
}

fn normalize_slashes(path: &str) -> String {
    let mut out = path.replace('\\', "/");
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_kind_defaults_to_other() {
        assert_eq!(get_tool_kind("read_file"), "read");
        assert_eq!(get_tool_kind("nonexistent_tool_xyz"), "other");
    }

    #[test]
    fn title_samples_match_python_shapes() {
        assert_eq!(
            build_tool_title("execute_code", &json!({"code": "\nprint('done')"})),
            "python: print('done')"
        );
        assert_eq!(
            build_tool_title(
                "skill_view",
                &json!({"name": "github-pitfalls", "file_path": "references/api.md"})
            ),
            "skill view (github-pitfalls/references/api.md)"
        );
    }

    #[test]
    fn toolsets_expand_with_mcp_servers_without_duplicates() {
        assert_eq!(
            expand_acp_enabled_toolsets(&["hermes-acp", "hermes-acp"], &["fs", "fs", "git"]),
            vec!["hermes-acp", "mcp-fs", "mcp-git"]
        );
    }

    #[test]
    fn permission_mapping_defaults_to_deny() {
        assert_eq!(permission_result_for_kind("allow_once"), "once");
        assert_eq!(permission_result_for_kind("allow_always"), "always");
        assert_eq!(permission_result_for_kind("reject_once"), "deny");
        assert_eq!(permission_result_for_kind("unexpected"), "deny");
    }

    #[test]
    fn cwd_normalization_matches_wsl_drive_rules() {
        assert_eq!(
            normalize_cwd_for_compare(r"E:\Projects\AI\paperclip"),
            "/mnt/e/Projects/AI/paperclip"
        );
        assert_eq!(
            normalize_cwd_for_compare("/mnt/E/Projects/AI/paperclip"),
            "/mnt/e/Projects/AI/paperclip"
        );
    }
}

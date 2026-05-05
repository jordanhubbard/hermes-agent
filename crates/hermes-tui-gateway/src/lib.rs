use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{json, Map, Value};

const REQUIRED_ACCEPTANCE_METHODS: &[&str] = &[
    "prompt.submit",
    "slash.exec",
    "approval.respond",
    "complete.path",
    "complete.slash",
    "session.list",
    "session.resume",
];

const LONG_HANDLERS: &[&str] = &[
    "browser.manage",
    "cli.exec",
    "session.branch",
    "session.compress",
    "session.resume",
    "shell.exec",
    "skills.manage",
    "slash.exec",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TuiMethodContract {
    pub name: String,
    pub group: String,
    pub params: Vec<String>,
    pub result_fields: Vec<String>,
    pub emits: Vec<String>,
    pub error_codes: Vec<i32>,
    pub long_handler: bool,
    pub runtime: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TuiEventContract {
    pub event_type: String,
    pub payload_fields: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EventSequence {
    pub name: String,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JsonRpcCase {
    pub name: String,
    pub request: Value,
    pub response: Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeBoundary {
    pub surface: String,
    pub boundary: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TuiProtocolSnapshot {
    pub protocol: String,
    pub methods: Vec<TuiMethodContract>,
    pub events: Vec<TuiEventContract>,
    pub long_handlers: Vec<String>,
    pub required_acceptance_methods: Vec<String>,
    pub stream_sequences: Vec<EventSequence>,
    pub json_rpc_cases: Vec<JsonRpcCase>,
    pub runtime_boundaries: Vec<RuntimeBoundary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedRequest {
    Request {
        id: Value,
        method: String,
        params: Map<String, Value>,
    },
    Error(Value),
}

pub fn tui_protocol_snapshot() -> TuiProtocolSnapshot {
    TuiProtocolSnapshot {
        protocol: "tui_gateway.jsonrpc.v1".to_string(),
        methods: method_contracts(),
        events: event_contracts(),
        long_handlers: strings(LONG_HANDLERS),
        required_acceptance_methods: strings(REQUIRED_ACCEPTANCE_METHODS),
        stream_sequences: vec![
            EventSequence {
                name: "prompt.submit.success".to_string(),
                events: strings(&["message.start", "message.delta", "message.complete"]),
            },
            EventSequence {
                name: "prompt.submit.agent_init_error".to_string(),
                events: strings(&["error"]),
            },
            EventSequence {
                name: "tool.call.visible".to_string(),
                events: strings(&["tool.start", "tool.progress", "tool.complete"]),
            },
            EventSequence {
                name: "approval.prompt".to_string(),
                events: strings(&["approval.request"]),
            },
            EventSequence {
                name: "blocking.prompts".to_string(),
                events: strings(&["clarify.request", "sudo.request", "secret.request"]),
            },
        ],
        json_rpc_cases: json_rpc_cases(),
        runtime_boundaries: vec![
            RuntimeBoundary {
                surface: "agent runtime".to_string(),
                boundary: "python_runtime_binding".to_string(),
                reason: "The TUI JSON-RPC transport and wire contract are represented in Rust; live AIAgent execution, slash-worker subprocess state, and prompt callbacks remain bound to the Python gateway until the agent runtime is cut over.".to_string(),
            },
            RuntimeBoundary {
                surface: "Ink frontend".to_string(),
                boundary: "typescript_client_contract".to_string(),
                reason: "Rust protocol snapshots are parity-tested against ui-tui request sites and GatewayEvent types so the frontend can move between Python-bound and Rust-backed gateways without a wire change.".to_string(),
            },
        ],
    }
}

pub fn normalize_request(req: &Value) -> NormalizedRequest {
    let Some(obj) = req.as_object() else {
        return NormalizedRequest::Error(err(
            Value::Null,
            -32600,
            "invalid request: expected an object",
        ));
    };

    let id = obj.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return NormalizedRequest::Error(err(
            id,
            -32600,
            "invalid request: method must be a non-empty string",
        ));
    };
    if method.is_empty() {
        return NormalizedRequest::Error(err(
            id,
            -32600,
            "invalid request: method must be a non-empty string",
        ));
    }

    let params = match obj.get("params") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return NormalizedRequest::Error(err(id, -32602, "invalid params: expected an object"))
        }
    };

    NormalizedRequest::Request {
        id,
        method: method.to_string(),
        params,
    }
}

pub fn dispatch_protocol_only(req: &Value) -> Value {
    match normalize_request(req) {
        NormalizedRequest::Error(response) => response,
        NormalizedRequest::Request { id, method, .. } => {
            if method_names().contains(method.as_str()) {
                ok(id, json!({"status": "accepted"}))
            } else {
                err(id, -32601, format!("unknown method: {method}"))
            }
        }
    }
}

pub fn event_frame(event_type: &str, session_id: &str, payload: Option<Value>) -> Value {
    let mut params = Map::new();
    params.insert("type".to_string(), Value::String(event_type.to_string()));
    params.insert(
        "session_id".to_string(),
        Value::String(session_id.to_string()),
    );
    if let Some(payload) = payload {
        params.insert("payload".to_string(), payload);
    }
    json!({"jsonrpc": "2.0", "method": "event", "params": params})
}

pub fn prompt_stream_frames(session_id: &str, deltas: &[&str], final_text: &str) -> Vec<Value> {
    let mut frames = vec![event_frame("message.start", session_id, None)];
    for delta in deltas {
        frames.push(event_frame(
            "message.delta",
            session_id,
            Some(json!({"text": delta})),
        ));
    }
    frames.push(event_frame(
        "message.complete",
        session_id,
        Some(json!({"text": final_text, "status": "complete"})),
    ));
    frames
}

pub fn method_names() -> BTreeSet<&'static str> {
    METHOD_NAMES.iter().copied().collect()
}

pub fn event_types() -> BTreeSet<&'static str> {
    EVENT_TYPES.iter().copied().collect()
}

fn method_contracts() -> Vec<TuiMethodContract> {
    vec![
        method(
            "session.create",
            "session",
            &["cols"],
            &["session_id", "info"],
            &["session.info"],
            &[5000],
            "python_bound",
        ),
        method(
            "session.list",
            "session",
            &["limit"],
            &["sessions"],
            &[],
            &[5006],
            "python_bound",
        ),
        method(
            "session.most_recent",
            "session",
            &[],
            &["session_id", "title", "started_at", "source"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "session.resume",
            "session",
            &["session_id", "cols"],
            &["session_id", "resumed", "message_count", "messages", "info"],
            &["session.info"],
            &[4006, 4007, 5000],
            "python_bound",
        ),
        method(
            "session.delete",
            "session",
            &["session_id"],
            &["deleted"],
            &[],
            &[4006, 4007, 4023, 5036],
            "python_bound",
        ),
        method(
            "session.title",
            "session",
            &["session_id", "title"],
            &["title", "session_key", "pending"],
            &[],
            &[4001, 4021, 5007],
            "python_bound",
        ),
        method(
            "session.usage",
            "session",
            &["session_id"],
            &["model", "calls", "input", "output", "total", "cost_usd"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "session.history",
            "session",
            &["session_id"],
            &["messages"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "session.undo",
            "session",
            &["session_id"],
            &["removed"],
            &["session.info"],
            &[4001, 4009],
            "python_bound",
        ),
        method(
            "session.compress",
            "session",
            &["session_id", "mode"],
            &[
                "before_messages",
                "after_messages",
                "removed",
                "summary",
                "messages",
                "info",
            ],
            &["session.info", "status.update"],
            &[4001, 4009],
            "python_bound",
        ),
        method(
            "session.save",
            "session",
            &["session_id"],
            &["file"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "session.close",
            "session",
            &["session_id"],
            &["ok"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "session.branch",
            "session",
            &["session_id", "name"],
            &["session_id", "title"],
            &["session.info"],
            &[4001],
            "python_bound",
        ),
        method(
            "session.interrupt",
            "session",
            &["session_id"],
            &["ok"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "delegation.status",
            "delegation",
            &[],
            &[
                "active",
                "paused",
                "max_concurrent_children",
                "max_spawn_depth",
            ],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "delegation.pause",
            "delegation",
            &["paused"],
            &["paused"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "subagent.interrupt",
            "delegation",
            &["subagent_id"],
            &["found", "subagent_id"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "spawn_tree.save",
            "delegation",
            &["session_id", "label"],
            &["path"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "spawn_tree.list",
            "delegation",
            &["session_id"],
            &["entries"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "spawn_tree.load",
            "delegation",
            &["path"],
            &[
                "session_id",
                "label",
                "started_at",
                "finished_at",
                "subagents",
            ],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "session.steer",
            "session",
            &["session_id", "text"],
            &["status", "text"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "terminal.resize",
            "terminal",
            &["session_id", "cols"],
            &["cols"],
            &[],
            &[4001],
            "rust_protocol_python_runtime",
        ),
        method(
            "prompt.submit",
            "prompt",
            &["session_id", "text"],
            &["status"],
            &[
                "message.start",
                "message.delta",
                "message.complete",
                "error",
                "status.update",
            ],
            &[4001, 4009],
            "python_bound",
        ),
        method(
            "clipboard.paste",
            "input",
            &["session_id"],
            &[
                "attached",
                "path",
                "count",
                "message",
                "width",
                "height",
                "token_estimate",
            ],
            &[],
            &[4001, 5027],
            "python_bound",
        ),
        method(
            "image.attach",
            "input",
            &["session_id", "path"],
            &[
                "attached",
                "path",
                "count",
                "remainder",
                "text",
                "width",
                "height",
                "token_estimate",
            ],
            &[],
            &[4001, 4015, 4016, 5027],
            "python_bound",
        ),
        method(
            "input.detect_drop",
            "input",
            &["session_id", "text"],
            &["matched", "is_image", "path", "count", "name", "text"],
            &[],
            &[4001, 5027],
            "python_bound",
        ),
        method(
            "prompt.background",
            "prompt",
            &["session_id", "text"],
            &["task_id"],
            &["background.complete"],
            &[4001, 4012],
            "python_bound",
        ),
        method(
            "clarify.respond",
            "prompt",
            &["request_id", "answer"],
            &["status"],
            &[],
            &[4009],
            "rust_protocol_python_runtime",
        ),
        method(
            "sudo.respond",
            "prompt",
            &["request_id", "password"],
            &["status"],
            &[],
            &[4009],
            "rust_protocol_python_runtime",
        ),
        method(
            "secret.respond",
            "prompt",
            &["request_id", "value"],
            &["status"],
            &[],
            &[4009],
            "rust_protocol_python_runtime",
        ),
        method(
            "approval.respond",
            "approval",
            &["session_id", "choice", "all"],
            &["resolved"],
            &[],
            &[4001],
            "python_bound",
        ),
        method(
            "config.set",
            "config",
            &["key", "value", "session_id"],
            &[
                "value",
                "warning",
                "credential_warning",
                "history_reset",
                "info",
            ],
            &["session.info", "skin.changed"],
            &[4001, 4004],
            "python_bound",
        ),
        method(
            "config.get",
            "config",
            &["key"],
            &["value", "display", "home", "mtime", "config"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "setup.status",
            "config",
            &[],
            &["provider_configured"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "process.stop",
            "ops",
            &[],
            &["killed"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "reload.mcp",
            "ops",
            &["session_id"],
            &["status", "message"],
            &["session.info"],
            &[],
            "python_bound",
        ),
        method(
            "reload.env",
            "ops",
            &[],
            &["updated"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "commands.catalog",
            "commands",
            &[],
            &[
                "pairs",
                "sub",
                "canon",
                "categories",
                "skill_count",
                "warning",
            ],
            &[],
            &[5020],
            "python_bound",
        ),
        method(
            "cli.exec",
            "commands",
            &["argv", "timeout"],
            &["blocked", "hint", "code", "output"],
            &[],
            &[4003, 5016, 5017],
            "python_bound",
        ),
        method(
            "command.resolve",
            "commands",
            &["name"],
            &["canonical", "description", "category"],
            &[],
            &[4011, 5012],
            "python_bound",
        ),
        method(
            "command.dispatch",
            "commands",
            &["session_id", "name", "arg"],
            &["type", "output", "target", "message", "name", "notice"],
            &[],
            &[4001, 4004, 4018, 5030],
            "python_bound",
        ),
        method(
            "paste.collapse",
            "input",
            &["text"],
            &["placeholder", "path", "lines"],
            &[],
            &[4004],
            "python_bound",
        ),
        method(
            "complete.path",
            "completion",
            &["word"],
            &["items"],
            &[],
            &[5021],
            "python_bound",
        ),
        method(
            "complete.slash",
            "completion",
            &["text"],
            &["items", "replace_from"],
            &[],
            &[5020],
            "python_bound",
        ),
        method(
            "model.options",
            "model",
            &["session_id"],
            &["providers", "model", "provider"],
            &[],
            &[5033],
            "python_bound",
        ),
        method(
            "model.save_key",
            "model",
            &["session_id", "slug", "api_key"],
            &["provider"],
            &[],
            &[4001, 4002, 4003, 4004, 4006, 5034],
            "python_bound",
        ),
        method(
            "model.disconnect",
            "model",
            &["slug"],
            &["slug", "name", "disconnected"],
            &[],
            &[4001, 4005, 5035],
            "python_bound",
        ),
        method(
            "slash.exec",
            "commands",
            &["session_id", "command"],
            &["output", "warning"],
            &["session.info"],
            &[4001, 4004, 4018, 5030],
            "python_bound",
        ),
        method(
            "voice.toggle",
            "voice",
            &["session_id", "action"],
            &[
                "enabled",
                "available",
                "audio_available",
                "stt_available",
                "tts",
                "details",
            ],
            &["voice.status"],
            &[4015],
            "python_bound",
        ),
        method(
            "voice.record",
            "voice",
            &["session_id", "action"],
            &["status", "text"],
            &["voice.status", "voice.transcript"],
            &[4015],
            "python_bound",
        ),
        method(
            "voice.tts",
            "voice",
            &["text"],
            &["status"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "insights.get",
            "ops",
            &["session_id"],
            &["items"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "rollback.list",
            "rollback",
            &["session_id"],
            &["enabled", "checkpoints"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "rollback.restore",
            "rollback",
            &["session_id", "hash", "file_path"],
            &[
                "success",
                "message",
                "restored_to",
                "history_removed",
                "error",
                "reason",
            ],
            &[],
            &[4009],
            "python_bound",
        ),
        method(
            "rollback.diff",
            "rollback",
            &["session_id", "hash"],
            &["diff", "rendered", "stat"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "browser.manage",
            "browser",
            &["session_id", "action", "url"],
            &["connected", "messages", "url"],
            &["browser.progress"],
            &[],
            "python_bound",
        ),
        method(
            "plugins.list",
            "plugins",
            &[],
            &["plugins"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "config.show",
            "config",
            &[],
            &["text"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "tools.list",
            "tools",
            &[],
            &["tools"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "tools.show",
            "tools",
            &["name"],
            &["text"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "tools.configure",
            "tools",
            &["session_id", "action", "names"],
            &[
                "changed",
                "unknown",
                "missing_servers",
                "enabled_toolsets",
                "reset",
                "info",
            ],
            &["session.info"],
            &[],
            "python_bound",
        ),
        method(
            "toolsets.list",
            "tools",
            &[],
            &["toolsets"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "agents.list",
            "agents",
            &[],
            &["agents"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "cron.manage",
            "cron",
            &["action", "job_id"],
            &["output"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "skills.manage",
            "skills",
            &["action", "query", "page"],
            &["skills", "info", "installed", "name", "results", "page"],
            &[],
            &[],
            "python_bound",
        ),
        method(
            "shell.exec",
            "terminal",
            &["command"],
            &["stdout", "stderr", "code"],
            &[],
            &[],
            "python_bound",
        ),
    ]
}

fn event_contracts() -> Vec<TuiEventContract> {
    vec![
        event("gateway.ready", &["skin"], "entry"),
        event(
            "skin.changed",
            &[
                "banner_hero",
                "banner_logo",
                "branding",
                "colors",
                "help_header",
                "tool_prefix",
            ],
            "server",
        ),
        event(
            "session.info",
            &["model", "tools", "skills", "cwd"],
            "server",
        ),
        event("thinking.delta", &["text"], "agent_callback"),
        event("message.start", &[], "prompt_submit"),
        event("status.update", &["kind", "text"], "server"),
        event("voice.status", &["state"], "voice"),
        event("voice.transcript", &["text", "no_speech_limit"], "voice"),
        event("gateway.stderr", &["line"], "frontend_gateway_client"),
        event("browser.progress", &["level", "message"], "browser"),
        event(
            "gateway.start_timeout",
            &["cwd", "python", "stderr_tail"],
            "frontend_gateway_client",
        ),
        event(
            "gateway.protocol_error",
            &["preview"],
            "frontend_gateway_client",
        ),
        event("reasoning.delta", &["text"], "agent_callback"),
        event("reasoning.available", &["text"], "agent_callback"),
        event("tool.progress", &["name", "preview"], "tool_callback"),
        event("tool.generating", &["name"], "tool_callback"),
        event(
            "tool.start",
            &["tool_id", "name", "context", "todos"],
            "tool_callback",
        ),
        event(
            "tool.complete",
            &[
                "tool_id",
                "name",
                "summary",
                "duration_s",
                "inline_diff",
                "todos",
                "error",
            ],
            "tool_callback",
        ),
        event(
            "clarify.request",
            &["request_id", "question", "choices"],
            "prompt_callback",
        ),
        event(
            "approval.request",
            &["command", "description"],
            "approval_callback",
        ),
        event("sudo.request", &["request_id"], "prompt_callback"),
        event(
            "secret.request",
            &["request_id", "env_var", "prompt"],
            "prompt_callback",
        ),
        event(
            "background.complete",
            &["task_id", "text"],
            "prompt_background",
        ),
        event("review.summary", &["text"], "review_callback"),
        event(
            "subagent.spawn_requested",
            &[
                "goal",
                "task_count",
                "task_index",
                "subagent_id",
                "parent_id",
                "depth",
            ],
            "delegate_callback",
        ),
        event(
            "subagent.start",
            &[
                "goal",
                "task_count",
                "task_index",
                "subagent_id",
                "parent_id",
                "depth",
                "model",
            ],
            "delegate_callback",
        ),
        event(
            "subagent.thinking",
            &["goal", "task_count", "task_index", "text"],
            "delegate_callback",
        ),
        event(
            "subagent.tool",
            &[
                "goal",
                "task_count",
                "task_index",
                "tool_name",
                "tool_preview",
                "text",
            ],
            "delegate_callback",
        ),
        event(
            "subagent.progress",
            &["goal", "task_count", "task_index", "text", "status"],
            "delegate_callback",
        ),
        event(
            "subagent.complete",
            &[
                "goal",
                "task_count",
                "task_index",
                "status",
                "summary",
                "duration_seconds",
                "files_read",
                "files_written",
            ],
            "delegate_callback",
        ),
        event("message.delta", &["text", "rendered"], "prompt_submit"),
        event(
            "message.complete",
            &[
                "text",
                "rendered",
                "reasoning",
                "usage",
                "status",
                "warning",
            ],
            "prompt_submit",
        ),
        event("error", &["message"], "server"),
    ]
}

const METHOD_NAMES: &[&str] = &[
    "session.create",
    "session.list",
    "session.most_recent",
    "session.resume",
    "session.delete",
    "session.title",
    "session.usage",
    "session.history",
    "session.undo",
    "session.compress",
    "session.save",
    "session.close",
    "session.branch",
    "session.interrupt",
    "delegation.status",
    "delegation.pause",
    "subagent.interrupt",
    "spawn_tree.save",
    "spawn_tree.list",
    "spawn_tree.load",
    "session.steer",
    "terminal.resize",
    "prompt.submit",
    "clipboard.paste",
    "image.attach",
    "input.detect_drop",
    "prompt.background",
    "clarify.respond",
    "sudo.respond",
    "secret.respond",
    "approval.respond",
    "config.set",
    "config.get",
    "setup.status",
    "process.stop",
    "reload.mcp",
    "reload.env",
    "commands.catalog",
    "cli.exec",
    "command.resolve",
    "command.dispatch",
    "paste.collapse",
    "complete.path",
    "complete.slash",
    "model.options",
    "model.save_key",
    "model.disconnect",
    "slash.exec",
    "voice.toggle",
    "voice.record",
    "voice.tts",
    "insights.get",
    "rollback.list",
    "rollback.restore",
    "rollback.diff",
    "browser.manage",
    "plugins.list",
    "config.show",
    "tools.list",
    "tools.show",
    "tools.configure",
    "toolsets.list",
    "agents.list",
    "cron.manage",
    "skills.manage",
    "shell.exec",
];

const EVENT_TYPES: &[&str] = &[
    "gateway.ready",
    "skin.changed",
    "session.info",
    "thinking.delta",
    "message.start",
    "status.update",
    "voice.status",
    "voice.transcript",
    "gateway.stderr",
    "browser.progress",
    "gateway.start_timeout",
    "gateway.protocol_error",
    "reasoning.delta",
    "reasoning.available",
    "tool.progress",
    "tool.generating",
    "tool.start",
    "tool.complete",
    "clarify.request",
    "approval.request",
    "sudo.request",
    "secret.request",
    "background.complete",
    "review.summary",
    "subagent.spawn_requested",
    "subagent.start",
    "subagent.thinking",
    "subagent.tool",
    "subagent.progress",
    "subagent.complete",
    "message.delta",
    "message.complete",
    "error",
];

fn method(
    name: &str,
    group: &str,
    params: &[&str],
    result_fields: &[&str],
    emits: &[&str],
    error_codes: &[i32],
    runtime: &str,
) -> TuiMethodContract {
    TuiMethodContract {
        name: name.to_string(),
        group: group.to_string(),
        params: strings(params),
        result_fields: strings(result_fields),
        emits: strings(emits),
        error_codes: error_codes.to_vec(),
        long_handler: LONG_HANDLERS.contains(&name),
        runtime: runtime.to_string(),
    }
}

fn event(event_type: &str, payload_fields: &[&str], source: &str) -> TuiEventContract {
    TuiEventContract {
        event_type: event_type.to_string(),
        payload_fields: strings(payload_fields),
        source: source.to_string(),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn json_rpc_cases() -> Vec<JsonRpcCase> {
    let cases = vec![
        ("non_object", json!([])),
        ("missing_method", json!({"id": "1"})),
        ("empty_method", json!({"id": "2", "method": ""})),
        (
            "null_params",
            json!({"id": "3", "method": "session.list", "params": null}),
        ),
        (
            "object_params",
            json!({"id": "4", "method": "session.list", "params": {"limit": 5}}),
        ),
        (
            "array_params",
            json!({"id": "5", "method": "session.list", "params": []}),
        ),
        (
            "unknown_method",
            json!({"id": "6", "method": "bogus", "params": {}}),
        ),
    ];

    cases
        .into_iter()
        .map(|(name, request)| JsonRpcCase {
            name: name.to_string(),
            response: dispatch_protocol_only(&request),
            request,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_methods_are_in_catalog() {
        let names = method_names();
        for method in REQUIRED_ACCEPTANCE_METHODS {
            assert!(names.contains(method));
        }
    }

    #[test]
    fn long_handler_flags_match_catalog() {
        let snapshot = tui_protocol_snapshot();
        let flagged: BTreeSet<_> = snapshot
            .methods
            .iter()
            .filter(|method| method.long_handler)
            .map(|method| method.name.as_str())
            .collect();
        let expected: BTreeSet<_> = LONG_HANDLERS.iter().copied().collect();
        assert_eq!(flagged, expected);
    }

    #[test]
    fn event_frame_omits_missing_payload() {
        let frame = event_frame("message.start", "sid", None);
        assert_eq!(frame["method"], "event");
        assert_eq!(frame["params"]["type"], "message.start");
        assert!(frame["params"].get("payload").is_none());
    }

    #[test]
    fn prompt_stream_is_start_delta_complete() {
        let frames = prompt_stream_frames("sid", &["a", "b"], "ab");
        let types: Vec<_> = frames
            .iter()
            .map(|frame| frame["params"]["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            types,
            [
                "message.start",
                "message.delta",
                "message.delta",
                "message.complete"
            ]
        );
    }

    #[test]
    fn json_rpc_normalization_matches_python_contract() {
        assert_eq!(
            dispatch_protocol_only(&json!([])),
            json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32600, "message": "invalid request: expected an object"}})
        );
        assert_eq!(
            dispatch_protocol_only(&json!({"id": "5", "method": "session.list", "params": []})),
            json!({"jsonrpc": "2.0", "id": "5", "error": {"code": -32602, "message": "invalid params: expected an object"}})
        );
        assert_eq!(
            dispatch_protocol_only(&json!({"id": "6", "method": "bogus", "params": {}})),
            json!({"jsonrpc": "2.0", "id": "6", "error": {"code": -32601, "message": "unknown method: bogus"}})
        );
    }
}

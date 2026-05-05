use std::env;
use std::ffi::OsString;
use std::process;

use hermes_agent_core::{
    run_agent_runtime, AgentRuntimeConfig, AssistantTurn, ConversationOutcome, Message,
    ModelClient, NoopHooks, NoopStore, ParsedProviderResponse, ProviderHttpError,
    ProviderRequestOptions, RuntimeDeps, TokenUsage, ToolCall, ToolDefinition, ToolDispatcher,
    ToolResult,
};
use hermes_cli::launcher::{
    is_runtime_info_request, is_rust_agent_runtime_smoke_request, is_rust_help_request,
    is_rust_version_request, python_command, render_rust_help, render_rust_version, runtime_info,
    select_runtime, RuntimeSelection,
};
use serde_json::json;
use std::collections::VecDeque;

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let selection = match select_runtime(env::var("HERMES_RUNTIME").ok().as_deref()) {
        Ok(selection) => selection,
        Err(message) => {
            eprintln!("{message}");
            process::exit(64);
        }
    };

    if is_runtime_info_request(&args) {
        let info = runtime_info(selection, &args);
        println!(
            "{}",
            serde_json::to_string(&info).expect("runtime info serializes")
        );
        return;
    }

    let code = match selection {
        RuntimeSelection::Python => run_python(&args),
        RuntimeSelection::Rust => run_rust(&args),
    };
    process::exit(code);
}

fn run_python(args: &[OsString]) -> i32 {
    let mut command = match python_command(args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            return 127;
        }
    };

    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("failed to start Python runtime: {err}");
            127
        }
    }
}

fn run_rust(args: &[OsString]) -> i32 {
    if is_rust_help_request(args) {
        print!("{}", render_rust_help());
        return 0;
    }

    if is_rust_version_request(args) {
        println!("{}", render_rust_version());
        return 0;
    }

    if is_rust_agent_runtime_smoke_request(args) {
        return run_agent_runtime_smoke();
    }

    let command = args
        .first()
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_else(|| "chat".to_string());
    eprintln!(
        "HERMES_RUNTIME=rust selected, but command {command:?} is not Rust-owned yet. \
Use HERMES_RUNTIME=python for the rollout fallback. Full parity remains tracked by the hermes-fpr beads."
    );
    78
}

struct SmokeModel {
    responses: VecDeque<ParsedProviderResponse>,
}

impl ModelClient for SmokeModel {
    fn call(
        &mut self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _options: &ProviderRequestOptions,
    ) -> Result<ParsedProviderResponse, ProviderHttpError> {
        Ok(self
            .responses
            .pop_front()
            .expect("smoke response queue has enough responses"))
    }
}

#[derive(Default)]
struct SmokeTools {
    calls: u32,
}

impl ToolDispatcher for SmokeTools {
    fn dispatch(&mut self, call: &ToolCall) -> ToolResult {
        self.calls = self.calls.saturating_add(1);
        ToolResult {
            call_id: call.id.clone(),
            ok: true,
            content: json!({"tool": call.name, "ok": true}).to_string(),
        }
    }
}

fn run_agent_runtime_smoke() -> i32 {
    let tool_call = ToolCall {
        id: "smoke_call_1".to_string(),
        name: "smoke_echo".to_string(),
        arguments: json!({"value": "ping"}),
    };
    let mut model = SmokeModel {
        responses: VecDeque::from([
            ParsedProviderResponse {
                assistant: AssistantTurn {
                    content: None,
                    tool_calls: vec![tool_call],
                    reasoning: Some("exercise tool dispatch".to_string()),
                },
                usage: TokenUsage {
                    input_tokens: 4,
                    output_tokens: 2,
                    reasoning_tokens: 1,
                    ..TokenUsage::default()
                },
                finish_reason: None,
            },
            ParsedProviderResponse {
                assistant: AssistantTurn {
                    content: Some("rust runtime smoke ok".to_string()),
                    ..AssistantTurn::default()
                },
                usage: TokenUsage {
                    input_tokens: 3,
                    output_tokens: 5,
                    ..TokenUsage::default()
                },
                finish_reason: None,
            },
        ]),
    };
    let mut tools = SmokeTools::default();
    let mut store = NoopStore;
    let mut hooks = NoopHooks;
    let result = run_agent_runtime(
        vec![Message::user("run smoke")],
        &[],
        AgentRuntimeConfig::default(),
        RuntimeDeps {
            model: &mut model,
            fallback_model: None,
            tools: &mut tools,
            store: &mut store,
            hooks: &mut hooks,
        },
    );
    let final_message = match &result.outcome {
        ConversationOutcome::Completed { final_message } => final_message.as_str(),
        _ => "",
    };
    let ok = matches!(result.outcome, ConversationOutcome::Completed { .. })
        && final_message == "rust runtime smoke ok"
        && result.model_call_count == 2
        && result.tool_call_count == 1
        && tools.calls == 1;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": ok,
            "final_message": final_message,
            "model_call_count": result.model_call_count,
            "tool_call_count": result.tool_call_count,
            "message_count": result.messages.len(),
        }))
        .expect("smoke output serializes")
    );

    if ok {
        0
    } else {
        1
    }
}

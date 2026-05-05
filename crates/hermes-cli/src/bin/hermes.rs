use std::env;
use std::ffi::{OsStr, OsString};
use std::process;

use hermes_agent_core::{
    run_agent_runtime, AgentRuntimeConfig, AssistantTurn, ConversationOutcome, Message,
    ModelClient, NoopHooks, NoopStore, ParsedProviderResponse, ProviderHttpError,
    ProviderRequestOptions, RuntimeDeps, TokenUsage, ToolCall, ToolDefinition, ToolDispatcher,
    ToolResult,
};
use hermes_cli::launcher::{
    is_runtime_info_request, is_rust_agent_runtime_smoke_request, is_rust_config_path_request,
    is_rust_gateway_status_request, is_rust_help_request, is_rust_logs_request,
    is_rust_profile_request, is_rust_version_request, python_command, render_rust_help,
    render_rust_version, runtime_info, select_runtime, RuntimeSelection,
};
use hermes_cli::{
    gateway_status, list_profiles, profile_status, render_gateway_status, render_profile_list,
    render_profile_show, render_profile_status, resolve_rust_profile_context, run_logs_command,
    set_active_profile, show_profile, RustProfileContext,
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
    let profile_context = match resolve_rust_profile_context(args) {
        Ok(context) => context,
        Err(message) => {
            eprintln!("Error: {message}");
            return 1;
        }
    };
    env::set_var("HERMES_HOME", &profile_context.hermes_home);
    let args = &profile_context.args;

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

    if is_rust_config_path_request(args) {
        let filename = if args.get(1).is_some_and(|arg| arg == OsStr::new("env-path")) {
            ".env"
        } else {
            "config.yaml"
        };
        println!("{}", profile_context.hermes_home.join(filename).display());
        return 0;
    }

    if is_rust_gateway_status_request(args) {
        let status = gateway_status(&profile_context.hermes_home);
        print!("{}", render_gateway_status(&status));
        return 0;
    }

    if is_rust_logs_request(args) {
        let outcome = run_logs_command(
            args,
            &profile_context.hermes_home,
            &profile_context.paths.display_hermes_home,
        );
        print!("{}", outcome.output);
        return outcome.exit_code;
    }

    if is_rust_profile_request(args) {
        return run_profile_command(&profile_context, args);
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

fn run_profile_command(context: &RustProfileContext, args: &[OsString]) -> i32 {
    let subcommand = args.get(1).map(|arg| arg.to_string_lossy().into_owned());
    match subcommand.as_deref() {
        None => {
            let status = profile_status(context);
            print!("{}", render_profile_status(&status));
            0
        }
        Some("list") => {
            let profiles = list_profiles(context);
            print!(
                "{}",
                render_profile_list(&profiles, &context.active_profile)
            );
            0
        }
        Some("show") => {
            let Some(name) = args.get(2).map(|arg| arg.to_string_lossy().into_owned()) else {
                eprintln!("usage: hermes profile show <profile_name>");
                return 2;
            };
            match show_profile(context, &name) {
                Ok(profile) => {
                    print!("{}", render_profile_show(&profile));
                    0
                }
                Err(message) => {
                    println!("Error: {message}");
                    1
                }
            }
        }
        Some("use") => {
            let Some(name) = args.get(2).map(|arg| arg.to_string_lossy().into_owned()) else {
                eprintln!("usage: hermes profile use <profile_name>");
                return 2;
            };
            match set_active_profile(context, &name) {
                Ok(message) => {
                    print!("{message}");
                    0
                }
                Err(message) => {
                    println!("Error: {message}");
                    1
                }
            }
        }
        Some(_) => {
            let command = args
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!(
                "HERMES_RUNTIME=rust selected, but command {command:?} is not Rust-owned yet. \
Use HERMES_RUNTIME=python for the rollout fallback. Full parity remains tracked by the hermes-fpr beads."
            );
            78
        }
    }
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

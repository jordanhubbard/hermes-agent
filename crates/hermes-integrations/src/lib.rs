use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CronContract {
    pub storage_paths: Vec<String>,
    pub schedule_kinds: Vec<String>,
    pub job_api: Vec<String>,
    pub scheduler_api: Vec<String>,
    pub delivery_modes: Vec<String>,
    pub known_delivery_platforms: Vec<String>,
    pub home_target_env_vars: Vec<String>,
    pub rust_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BatchContract {
    pub cli_args: Vec<String>,
    pub runner_init_args: Vec<String>,
    pub output_files: Vec<String>,
    pub result_fields: Vec<String>,
    pub rust_boundary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct McpContract {
    pub server_name: String,
    pub tools: Vec<String>,
    pub event_types: Vec<String>,
    pub queue_limit: usize,
    pub poll_interval_seconds: f64,
    pub transport: String,
    pub rust_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RlContract {
    pub cli_args: Vec<String>,
    pub required_env: Vec<String>,
    pub toolsets: Vec<String>,
    pub max_iterations: usize,
    pub terminal_env: Vec<String>,
    pub agent_kwargs: Vec<String>,
    pub rust_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PluginContract {
    pub context_methods: Vec<String>,
    pub valid_hooks: Vec<String>,
    pub manifest_fields: Vec<String>,
    pub discovery_sources: Vec<String>,
    pub dashboard_helpers: Vec<String>,
    pub rust_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeBoundary {
    pub surface: String,
    pub boundary: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IntegrationsSnapshot {
    pub protocol: String,
    pub cron: CronContract,
    pub batch: BatchContract,
    pub mcp: McpContract,
    pub rl: RlContract,
    pub plugins: PluginContract,
    pub runtime_boundaries: Vec<RuntimeBoundary>,
}

pub fn integrations_snapshot() -> IntegrationsSnapshot {
    IntegrationsSnapshot {
        protocol: "hermes-integrations.boundaries.v1".to_string(),
        cron: cron_contract(),
        batch: batch_contract(),
        mcp: mcp_contract(),
        rl: rl_contract(),
        plugins: plugin_contract(),
        runtime_boundaries: vec![
            RuntimeBoundary {
                surface: "cron".to_string(),
                boundary: "python_scheduler_rust_contract".to_string(),
                reason: "Cron owns file persistence, gateway delivery, live adapter dispatch, and AIAgent invocation in Python. Rust parity locks the schedule/job/delivery contract while shared state and tool registry cutovers happen independently.".to_string(),
            },
            RuntimeBoundary {
                surface: "batch_runner".to_string(),
                boundary: "python_multiprocessing_rust_contract".to_string(),
                reason: "Batch multiprocessing, dataset IO, and trajectory assembly remain Python-bound; Rust validates the CLI, result schema, and output/checkpoint boundary used by training pipelines.".to_string(),
            },
            RuntimeBoundary {
                surface: "mcp".to_string(),
                boundary: "python_fastmcp_stdio_rust_contract".to_string(),
                reason: "The MCP SDK integration is a dynamic Python FastMCP server. Rust owns the stable tool/event surface contract until a Rust MCP server can reuse the same gateway/session backends.".to_string(),
            },
            RuntimeBoundary {
                surface: "rl".to_string(),
                boundary: "python_tinker_atropos_rust_contract".to_string(),
                reason: "RL workflows depend on Python Tinker-Atropos environments and async RL tools; Rust parity captures the CLI setup and AIAgent invocation contract without rewriting the training stack.".to_string(),
            },
            RuntimeBoundary {
                surface: "plugins".to_string(),
                boundary: "dynamic_python_plugin_api_rust_contract".to_string(),
                reason: "Plugins are intentionally dynamic Python modules and entry points. Rust parity locks the public registration facade, hooks, manifest fields, discovery sources, and dashboard helper API.".to_string(),
            },
        ],
    }
}

fn cron_contract() -> CronContract {
    CronContract {
        storage_paths: strings(&[
            "cron/jobs.json",
            "cron/output/{job_id}/{timestamp}.md",
            "cron/.tick.lock",
        ]),
        schedule_kinds: strings(&["once", "interval", "cron"]),
        job_api: strings(&[
            "ensure_dirs",
            "parse_duration",
            "parse_schedule",
            "compute_next_run",
            "load_jobs",
            "save_jobs",
            "create_job",
            "get_job",
            "list_jobs",
            "update_job",
            "pause_job",
            "resume_job",
            "trigger_job",
            "remove_job",
            "mark_job_run",
            "advance_next_run",
            "get_due_jobs",
            "save_job_output",
            "rewrite_skill_refs",
        ]),
        scheduler_api: strings(&["run_job", "tick"]),
        delivery_modes: strings(&[
            "local",
            "origin",
            "platform",
            "platform:target",
            "comma-separated",
        ]),
        known_delivery_platforms: sorted_strings(&[
            "telegram",
            "discord",
            "slack",
            "whatsapp",
            "signal",
            "matrix",
            "mattermost",
            "homeassistant",
            "dingtalk",
            "feishu",
            "wecom",
            "wecom_callback",
            "weixin",
            "sms",
            "email",
            "webhook",
            "bluebubbles",
            "qqbot",
            "yuanbao",
        ]),
        home_target_env_vars: sorted_strings(&[
            "MATRIX_HOME_ROOM",
            "TELEGRAM_HOME_CHANNEL",
            "DISCORD_HOME_CHANNEL",
            "SLACK_HOME_CHANNEL",
            "SIGNAL_HOME_CHANNEL",
            "MATTERMOST_HOME_CHANNEL",
            "SMS_HOME_CHANNEL",
            "EMAIL_HOME_ADDRESS",
            "DINGTALK_HOME_CHANNEL",
            "FEISHU_HOME_CHANNEL",
            "WECOM_HOME_CHANNEL",
            "WEIXIN_HOME_CHANNEL",
            "BLUEBUBBLES_HOME_CHANNEL",
            "QQBOT_HOME_CHANNEL",
        ]),
        rust_boundary: "schedule/job/delivery contract; Python runtime for live execution"
            .to_string(),
    }
}

fn batch_contract() -> BatchContract {
    BatchContract {
        cli_args: strings(&[
            "dataset_file",
            "batch_size",
            "run_name",
            "distribution",
            "model",
            "api_key",
            "base_url",
            "max_turns",
            "num_workers",
            "resume",
            "verbose",
            "list_distributions",
            "ephemeral_system_prompt",
            "log_prefix_chars",
            "providers_allowed",
            "providers_ignored",
            "providers_order",
            "provider_sort",
            "max_tokens",
            "reasoning_effort",
            "reasoning_disabled",
            "prefill_messages_file",
            "max_samples",
        ]),
        runner_init_args: strings(&[
            "dataset_file",
            "batch_size",
            "run_name",
            "distribution",
            "max_iterations",
            "base_url",
            "api_key",
            "model",
            "num_workers",
            "verbose",
            "ephemeral_system_prompt",
            "log_prefix_chars",
            "providers_allowed",
            "providers_ignored",
            "providers_order",
            "provider_sort",
            "max_tokens",
            "reasoning_config",
            "prefill_messages",
            "max_samples",
        ]),
        output_files: strings(&[
            "data/{run_name}/batch_{batch_num}.jsonl",
            "data/{run_name}/checkpoint.json",
            "data/{run_name}/statistics.json",
        ]),
        result_fields: strings(&[
            "success",
            "prompt_index",
            "trajectory",
            "tool_stats",
            "reasoning_stats",
            "completed",
            "partial",
            "api_calls",
            "toolsets_used",
            "metadata",
        ]),
        rust_boundary: "trajectory/stat/checkpoint contract; Python multiprocessing runtime"
            .to_string(),
    }
}

fn mcp_contract() -> McpContract {
    McpContract {
        server_name: "hermes".to_string(),
        tools: strings(&[
            "conversations_list",
            "conversation_get",
            "messages_read",
            "attachments_fetch",
            "events_poll",
            "events_wait",
            "messages_send",
            "channels_list",
            "permissions_list_open",
            "permissions_respond",
        ]),
        event_types: strings(&["message", "approval_requested", "approval_resolved"]),
        queue_limit: 1000,
        poll_interval_seconds: 0.2,
        transport: "FastMCP stdio".to_string(),
        rust_boundary: "MCP tool/event contract; Python FastMCP runtime".to_string(),
    }
}

fn rl_contract() -> RlContract {
    RlContract {
        cli_args: strings(&[
            "task",
            "model",
            "api_key",
            "base_url",
            "max_iterations",
            "interactive",
            "list_environments",
            "check_server",
            "verbose",
            "save_trajectories",
        ]),
        required_env: strings(&["OPENROUTER_API_KEY", "TINKER_API_KEY", "WANDB_API_KEY"]),
        toolsets: strings(&["terminal", "web", "rl"]),
        max_iterations: 200,
        terminal_env: strings(&["TERMINAL_CWD", "HERMES_QUIET"]),
        agent_kwargs: strings(&[
            "base_url",
            "api_key",
            "model",
            "max_iterations",
            "enabled_toolsets",
            "save_trajectories",
            "verbose_logging",
            "quiet_mode",
            "ephemeral_system_prompt",
        ]),
        rust_boundary: "RL CLI/AIAgent invocation contract; Python Tinker-Atropos runtime"
            .to_string(),
    }
}

fn plugin_contract() -> PluginContract {
    PluginContract {
        context_methods: strings(&[
            "register_tool",
            "inject_message",
            "register_cli_command",
            "register_command",
            "dispatch_tool",
            "register_context_engine",
            "register_image_gen_provider",
            "register_platform",
            "register_hook",
            "register_skill",
        ]),
        valid_hooks: sorted_strings(&[
            "pre_tool_call",
            "post_tool_call",
            "transform_terminal_output",
            "transform_tool_result",
            "pre_llm_call",
            "post_llm_call",
            "pre_api_request",
            "post_api_request",
            "on_session_start",
            "on_session_end",
            "on_session_finalize",
            "on_session_reset",
            "subagent_stop",
            "pre_gateway_dispatch",
            "pre_approval_request",
            "post_approval_response",
        ]),
        manifest_fields: strings(&[
            "name",
            "version",
            "description",
            "author",
            "requires_env",
            "provides_tools",
            "provides_hooks",
            "source",
            "path",
            "kind",
            "key",
        ]),
        discovery_sources: strings(&["bundled", "user", "project", "entry_points"]),
        dashboard_helpers: strings(&[
            "dashboard_install_plugin",
            "dashboard_set_agent_plugin_enabled",
            "dashboard_update_user_plugin",
            "dashboard_remove_user_plugin",
        ]),
        rust_boundary: "plugin registration/dashboard contract; dynamic Python module runtime"
            .to_string(),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

fn sorted_strings(values: &[&str]) -> Vec<String> {
    let mut out = strings(values);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_contract_matches_openclaw_plus_channels_surface() {
        let mcp = mcp_contract();
        assert_eq!(mcp.tools.len(), 10);
        assert!(mcp.tools.contains(&"channels_list".to_string()));
        assert_eq!(mcp.queue_limit, 1000);
    }

    #[test]
    fn plugin_contract_exposes_registration_facade() {
        let plugins = plugin_contract();
        assert!(plugins
            .context_methods
            .contains(&"register_tool".to_string()));
        assert!(plugins
            .context_methods
            .contains(&"register_platform".to_string()));
        assert!(plugins
            .valid_hooks
            .contains(&"pre_gateway_dispatch".to_string()));
    }
}

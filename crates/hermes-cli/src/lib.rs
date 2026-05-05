use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

pub mod config_command;
pub mod display;
pub mod gateway_status;
pub mod integrations_status;
pub mod launcher;
pub mod logs;
pub mod plugins;
pub mod profile;
pub mod setup;
pub mod skills;

pub use config_command::{run_config_set_command, ConfigCommandOutcome};
pub use display::{
    builtin_skin_surfaces, logging_plan, render_status, CliStatusInput, LoggingPlan, SkinSurface,
};
pub use gateway_status::{
    gateway_status, render_gateway_status, run_gateway_stop_command, GatewayStatus,
};
pub use integrations_status::{cron_status, render_cron_status, CronStatus};
pub use logs::{run_logs_command, LogsOutcome};
pub use plugins::{run_plugins_command, PluginsOutcome};
pub use profile::{
    delete_profile_yes, list_profiles, profile_status, render_profile_list, render_profile_show,
    render_profile_status, resolve_rust_profile_context, set_active_profile, show_profile,
    ProfileInfo, ProfileStatus, RustProfileContext,
};
pub use setup::{
    apply_model_choice, determine_api_mode, provider_setup_def, secret_storage_plan,
    setup_snapshot, supports_same_provider_pool_setup, ModelChoicePlan, ProviderSetupDef,
    SecretStoragePlan, SetupSnapshot,
};
pub use skills::{run_skills_command, SkillsOutcome};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CommandDef {
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub aliases: &'static [&'static str],
    pub args_hint: &'static str,
    pub subcommands: &'static [&'static str],
    pub cli_only: bool,
    pub gateway_only: bool,
    pub gateway_config_gate: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SlashDispatch {
    pub original: String,
    pub command_name: String,
    pub canonical_name: String,
    pub args: String,
    pub is_gateway_known: bool,
}

#[derive(Debug, Serialize)]
pub struct RegistrySnapshot {
    pub registry: &'static [CommandDef],
    pub commands: BTreeMap<String, String>,
    pub commands_by_category: BTreeMap<String, BTreeMap<String, String>>,
    pub subcommands: BTreeMap<String, Vec<String>>,
    pub gateway_known_commands: Vec<String>,
    pub gateway_help_lines: Vec<String>,
    pub telegram_bot_commands: Vec<(String, String)>,
    pub slack_subcommand_map: BTreeMap<String, String>,
    pub dispatch_samples: BTreeMap<String, Option<SlashDispatch>>,
}

pub static COMMAND_REGISTRY: &[CommandDef] = &[
    CommandDef {
        name: "new",
        description: "Start a new session (fresh session ID + history)",
        category: "Session",
        aliases: &["reset"],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "clear",
        description: "Clear screen and start a new session",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "redraw",
        description: "Force a full UI repaint (recovers from terminal drift)",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "history",
        description: "Show conversation history",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "save",
        description: "Save the current conversation",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "retry",
        description: "Retry the last message (resend to agent)",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "undo",
        description: "Remove the last user/assistant exchange",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "title",
        description: "Set a title for the current session",
        category: "Session",
        aliases: &[],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "branch",
        description: "Branch the current session (explore a different path)",
        category: "Session",
        aliases: &["fork"],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "compress",
        description: "Manually compress conversation context",
        category: "Session",
        aliases: &[],
        args_hint: "[focus topic]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "rollback",
        description: "List or restore filesystem checkpoints",
        category: "Session",
        aliases: &[],
        args_hint: "[number]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "snapshot",
        description: "Create or restore state snapshots of Hermes config/state",
        category: "Session",
        aliases: &["snap"],
        args_hint: "[create|restore <id>|prune]",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "stop",
        description: "Kill all running background processes",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "approve",
        description: "Approve a pending dangerous command",
        category: "Session",
        aliases: &[],
        args_hint: "[session|always]",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "deny",
        description: "Deny a pending dangerous command",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "background",
        description: "Run a prompt in the background",
        category: "Session",
        aliases: &["bg", "btw"],
        args_hint: "<prompt>",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "agents",
        description: "Show active agents and running tasks",
        category: "Session",
        aliases: &["tasks"],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "queue",
        description: "Queue a prompt for the next turn (doesn't interrupt)",
        category: "Session",
        aliases: &["q"],
        args_hint: "<prompt>",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "steer",
        description: "Inject a message after the next tool call without interrupting",
        category: "Session",
        aliases: &[],
        args_hint: "<prompt>",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "goal",
        description: "Set a standing goal Hermes works on across turns until achieved",
        category: "Session",
        aliases: &[],
        args_hint: "[text | pause | resume | clear | status]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "status",
        description: "Show session info",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "profile",
        description: "Show active profile name and home directory",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "sethome",
        description: "Set this chat as the home channel",
        category: "Session",
        aliases: &["set-home"],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "resume",
        description: "Resume a previously-named session",
        category: "Session",
        aliases: &[],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "config",
        description: "Show current configuration",
        category: "Configuration",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "model",
        description: "Switch model for this session",
        category: "Configuration",
        aliases: &["provider"],
        args_hint: "[model] [--provider name] [--global]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "gquota",
        description: "Show Google Gemini Code Assist quota usage",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "personality",
        description: "Set a predefined personality",
        category: "Configuration",
        aliases: &[],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "statusbar",
        description: "Toggle the context/model status bar",
        category: "Configuration",
        aliases: &["sb"],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "verbose",
        description: "Cycle tool progress display: off -> new -> all -> verbose",
        category: "Configuration",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: Some("display.tool_progress_command"),
    },
    CommandDef {
        name: "footer",
        description: "Toggle gateway runtime-metadata footer on final replies",
        category: "Configuration",
        aliases: &[],
        args_hint: "[on|off|status]",
        subcommands: &["on", "off", "status"],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "yolo",
        description: "Toggle YOLO mode (skip all dangerous command approvals)",
        category: "Configuration",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "reasoning",
        description: "Manage reasoning effort and display",
        category: "Configuration",
        aliases: &[],
        args_hint: "[level|show|hide]",
        subcommands: &[
            "none", "minimal", "low", "medium", "high", "xhigh", "show", "hide", "on", "off",
        ],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "fast",
        description:
            "Toggle fast mode — OpenAI Priority Processing / Anthropic Fast Mode (Normal/Fast)",
        category: "Configuration",
        aliases: &[],
        args_hint: "[normal|fast|status]",
        subcommands: &["normal", "fast", "status", "on", "off"],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "skin",
        description: "Show or change the display skin/theme",
        category: "Configuration",
        aliases: &[],
        args_hint: "[name]",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "indicator",
        description: "Pick the TUI busy-indicator style",
        category: "Configuration",
        aliases: &[],
        args_hint: "[kaomoji|emoji|unicode|ascii]",
        subcommands: &["kaomoji", "emoji", "unicode", "ascii"],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "voice",
        description: "Toggle voice mode",
        category: "Configuration",
        aliases: &[],
        args_hint: "[on|off|tts|status]",
        subcommands: &["on", "off", "tts", "status"],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "busy",
        description: "Control what Enter does while Hermes is working",
        category: "Configuration",
        aliases: &[],
        args_hint: "[queue|steer|interrupt|status]",
        subcommands: &["queue", "steer", "interrupt", "status"],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "tools",
        description: "Manage tools: /tools [list|disable|enable] [name...]",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "[list|disable|enable] [name...]",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "toolsets",
        description: "List available toolsets",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "skills",
        description: "Search, install, inspect, or manage skills",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "",
        subcommands: &["search", "browse", "inspect", "install"],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "cron",
        description: "Manage scheduled tasks",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "[subcommand]",
        subcommands: &[
            "list", "add", "create", "edit", "pause", "resume", "run", "remove",
        ],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "curator",
        description: "Background skill maintenance (status, run, pin, archive)",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "[subcommand]",
        subcommands: &[
            "status", "run", "pause", "resume", "pin", "unpin", "restore",
        ],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "kanban",
        description: "Multi-profile collaboration board (tasks, links, comments)",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "[subcommand]",
        subcommands: &[
            "list", "ls", "show", "create", "assign", "link", "unlink", "claim", "comment",
            "complete", "block", "unblock", "archive", "tail", "dispatch", "context", "init", "gc",
        ],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "reload",
        description: "Reload .env variables into the running session",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "reload-mcp",
        description: "Reload MCP servers from config",
        category: "Tools & Skills",
        aliases: &["reload_mcp"],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "reload-skills",
        description: "Re-scan ~/.hermes/skills/ for newly installed or removed skills",
        category: "Tools & Skills",
        aliases: &["reload_skills"],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "browser",
        description: "Connect browser tools to your live Chrome via CDP",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "[connect|disconnect|status]",
        subcommands: &["connect", "disconnect", "status"],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "plugins",
        description: "List installed plugins and their status",
        category: "Tools & Skills",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "commands",
        description: "Browse all commands and skills (paginated)",
        category: "Info",
        aliases: &[],
        args_hint: "[page]",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "help",
        description: "Show available commands",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "restart",
        description: "Gracefully restart the gateway after draining active runs",
        category: "Session",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "usage",
        description: "Show token usage and rate limits for the current session",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "insights",
        description: "Show usage insights and analytics",
        category: "Info",
        aliases: &[],
        args_hint: "[days]",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "platforms",
        description: "Show gateway/messaging platform status",
        category: "Info",
        aliases: &["gateway"],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "copy",
        description: "Copy the last assistant response to clipboard",
        category: "Info",
        aliases: &[],
        args_hint: "[number]",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "paste",
        description: "Attach clipboard image from your clipboard",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "image",
        description: "Attach a local image file for your next prompt",
        category: "Info",
        aliases: &[],
        args_hint: "<path>",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "update",
        description: "Update Hermes Agent to the latest version",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: true,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "debug",
        description: "Upload debug report (system info + logs) and get shareable links",
        category: "Info",
        aliases: &[],
        args_hint: "",
        subcommands: &[],
        cli_only: false,
        gateway_only: false,
        gateway_config_gate: None,
    },
    CommandDef {
        name: "quit",
        description: "Exit the CLI",
        category: "Exit",
        aliases: &["exit"],
        args_hint: "",
        subcommands: &[],
        cli_only: true,
        gateway_only: false,
        gateway_config_gate: None,
    },
];

pub fn resolve_command(name: &str) -> Option<&'static CommandDef> {
    let clean = name.trim_start_matches('/').to_ascii_lowercase();
    if clean.is_empty() {
        return None;
    }
    COMMAND_REGISTRY
        .iter()
        .find(|cmd| cmd.name == clean || cmd.aliases.iter().any(|alias| *alias == clean.as_str()))
}

pub fn parse_slash_dispatch(input: &str) -> Option<SlashDispatch> {
    let trimmed = input.trim();
    let without_slash = trimmed.strip_prefix('/')?;
    let mut parts = without_slash.splitn(2, char::is_whitespace);
    let command_name = parts.next()?.to_ascii_lowercase();
    if command_name.is_empty() {
        return None;
    }
    let args = parts.next().unwrap_or("").trim_start().to_string();
    let command = resolve_command(&command_name)?;
    Some(SlashDispatch {
        original: trimmed.to_string(),
        command_name,
        canonical_name: command.name.to_string(),
        args,
        is_gateway_known: gateway_known_commands().contains(command.name)
            || command
                .aliases
                .iter()
                .any(|alias| gateway_known_commands().contains(*alias)),
    })
}

pub fn cli_commands() -> BTreeMap<String, String> {
    let mut commands = BTreeMap::new();
    for cmd in COMMAND_REGISTRY {
        if cmd.gateway_only {
            continue;
        }
        commands.insert(format!("/{}", cmd.name), build_description(cmd));
        for alias in cmd.aliases {
            commands.insert(
                format!("/{}", alias),
                format!("{} (alias for /{})", cmd.description, cmd.name),
            );
        }
    }
    commands
}

pub fn commands_by_category() -> BTreeMap<String, BTreeMap<String, String>> {
    let command_descriptions = cli_commands();
    let mut by_category: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for cmd in COMMAND_REGISTRY {
        if cmd.gateway_only {
            continue;
        }
        let category = by_category.entry(cmd.category.to_string()).or_default();
        let key = format!("/{}", cmd.name);
        if let Some(desc) = command_descriptions.get(&key) {
            category.insert(key, desc.clone());
        }
        for alias in cmd.aliases {
            let key = format!("/{}", alias);
            if let Some(desc) = command_descriptions.get(&key) {
                category.insert(key, desc.clone());
            }
        }
    }
    by_category
}

pub fn subcommands() -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    for cmd in COMMAND_REGISTRY {
        if !cmd.subcommands.is_empty() {
            result.insert(
                format!("/{}", cmd.name),
                cmd.subcommands.iter().map(|s| (*s).to_string()).collect(),
            );
            continue;
        }
        if let Some(pipe_group) = pipe_subcommands_from_args_hint(cmd.args_hint) {
            result.insert(format!("/{}", cmd.name), pipe_group);
        }
    }
    result
}

pub fn gateway_known_commands() -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for cmd in COMMAND_REGISTRY {
        if !cmd.cli_only || cmd.gateway_config_gate.is_some() {
            result.insert(cmd.name.to_string());
            for alias in cmd.aliases {
                result.insert((*alias).to_string());
            }
        }
    }
    result
}

pub fn gateway_help_lines(config_gates: &BTreeSet<String>) -> Vec<String> {
    let mut lines = Vec::new();
    for cmd in COMMAND_REGISTRY {
        if !is_gateway_available(cmd, config_gates) {
            continue;
        }
        let args = if cmd.args_hint.is_empty() {
            String::new()
        } else {
            format!(" {}", cmd.args_hint)
        };
        let aliases: Vec<String> = cmd
            .aliases
            .iter()
            .filter(|alias| !normalized_same(alias, cmd.name))
            .map(|alias| format!("`/{}`", alias))
            .collect();
        let alias_note = if aliases.is_empty() {
            String::new()
        } else {
            format!(" (alias: {})", aliases.join(", "))
        };
        lines.push(format!(
            "`/{}{}` -- {}{}",
            cmd.name, args, cmd.description, alias_note
        ));
    }
    lines
}

pub fn telegram_bot_commands(config_gates: &BTreeSet<String>) -> Vec<(String, String)> {
    COMMAND_REGISTRY
        .iter()
        .filter(|cmd| is_gateway_available(cmd, config_gates))
        .filter(|cmd| !requires_argument(cmd.args_hint))
        .filter_map(|cmd| {
            let name = sanitize_telegram_name(cmd.name);
            if name.is_empty() {
                None
            } else {
                Some((name, cmd.description.to_string()))
            }
        })
        .collect()
}

pub fn slack_subcommand_map(config_gates: &BTreeSet<String>) -> BTreeMap<String, String> {
    let mut mapping = BTreeMap::new();
    for cmd in COMMAND_REGISTRY {
        if !is_gateway_available(cmd, config_gates) {
            continue;
        }
        mapping.insert(cmd.name.to_string(), format!("/{}", cmd.name));
        for alias in cmd.aliases {
            mapping.insert((*alias).to_string(), format!("/{}", alias));
        }
    }
    mapping
}

pub fn registry_snapshot(config_gates: &BTreeSet<String>) -> RegistrySnapshot {
    RegistrySnapshot {
        registry: COMMAND_REGISTRY,
        commands: cli_commands(),
        commands_by_category: commands_by_category(),
        subcommands: subcommands(),
        gateway_known_commands: gateway_known_commands().into_iter().collect(),
        gateway_help_lines: gateway_help_lines(config_gates),
        telegram_bot_commands: telegram_bot_commands(config_gates),
        slack_subcommand_map: slack_subcommand_map(config_gates),
        dispatch_samples: dispatch_samples(),
    }
}

pub fn dispatch_samples() -> BTreeMap<String, Option<SlashDispatch>> {
    [
        "/bg ship it",
        "/reset",
        "/reload_mcp",
        "/clear",
        "/help",
        "/unknown",
        "plain user message",
    ]
    .into_iter()
    .map(|sample| (sample.to_string(), parse_slash_dispatch(sample)))
    .collect()
}

fn build_description(cmd: &CommandDef) -> String {
    if cmd.args_hint.is_empty() {
        cmd.description.to_string()
    } else {
        format!(
            "{} (usage: /{} {})",
            cmd.description, cmd.name, cmd.args_hint
        )
    }
}

fn is_gateway_available(cmd: &CommandDef, config_gates: &BTreeSet<String>) -> bool {
    if !cmd.cli_only {
        return true;
    }
    cmd.gateway_config_gate
        .is_some_and(|_| config_gates.contains(cmd.name))
}

fn requires_argument(args_hint: &str) -> bool {
    args_hint.trim_start().starts_with('<')
}

fn normalized_same(left: &str, right: &str) -> bool {
    left.replace('-', "_") == right.replace('-', "_") && left != right
}

fn sanitize_telegram_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut previous_underscore = false;
    for ch in raw.to_ascii_lowercase().replace('-', "_").chars() {
        let keep = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_';
        if !keep {
            continue;
        }
        if ch == '_' {
            if previous_underscore {
                continue;
            }
            previous_underscore = true;
        } else {
            previous_underscore = false;
        }
        out.push(ch);
    }
    out.trim_matches('_').to_string()
}

fn pipe_subcommands_from_args_hint(args_hint: &str) -> Option<Vec<String>> {
    for token in args_hint.split(|ch: char| !(ch.is_ascii_lowercase() || ch == '|')) {
        if token.contains('|')
            && token
                .split('|')
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_lowercase()))
        {
            return Some(token.split('|').map(str::to_string).collect());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve_to_canonical_commands() {
        assert_eq!(resolve_command("bg").unwrap().name, "background");
        assert_eq!(resolve_command("/reset").unwrap().name, "new");
        assert_eq!(resolve_command("reload_mcp").unwrap().name, "reload-mcp");
        assert!(resolve_command("nonexistent").is_none());
    }

    #[test]
    fn gateway_known_commands_include_gateway_and_gated_aliases() {
        let known = gateway_known_commands();
        assert!(known.contains("background"));
        assert!(known.contains("bg"));
        assert!(known.contains("verbose"));
        assert!(!known.contains("clear"));
    }

    #[test]
    fn slash_dispatch_preserves_args_and_canonical_name() {
        let dispatch = parse_slash_dispatch("/bg ship it").unwrap();
        assert_eq!(dispatch.command_name, "bg");
        assert_eq!(dispatch.canonical_name, "background");
        assert_eq!(dispatch.args, "ship it");
        assert!(dispatch.is_gateway_known);
    }

    #[test]
    fn telegram_names_are_sanitized_and_required_args_are_skipped() {
        let commands = telegram_bot_commands(&BTreeSet::new());
        let names: BTreeSet<_> = commands.into_iter().map(|(name, _)| name).collect();
        assert!(names.contains("reload_mcp"));
        assert!(!names.contains("background"));
    }

    #[test]
    fn config_gate_controls_gateway_visible_help_surfaces() {
        let without = BTreeSet::new();
        assert!(!gateway_help_lines(&without)
            .iter()
            .any(|line| line.starts_with("`/verbose")));

        let mut with = BTreeSet::new();
        with.insert("verbose".to_string());
        assert!(gateway_help_lines(&with)
            .iter()
            .any(|line| line.starts_with("`/verbose")));
    }
}

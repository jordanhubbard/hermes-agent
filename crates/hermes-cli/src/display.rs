use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SkinSurface {
    pub name: &'static str,
    pub description: &'static str,
    pub tool_prefix: &'static str,
    pub banner_title: &'static str,
    pub response_border: &'static str,
    pub status_bar_bg: &'static str,
    pub agent_name: &'static str,
    pub response_label: &'static str,
    pub prompt_symbol: &'static str,
    pub help_header: &'static str,
    pub spinner_wing_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CliStatusInput {
    pub session_id: String,
    pub display_path: String,
    pub title: String,
    pub model: String,
    pub provider: String,
    pub created_at: String,
    pub last_activity: String,
    pub total_tokens: u64,
    pub agent_running: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LoggingPlan {
    pub log_dir: String,
    pub agent_log: String,
    pub errors_log: String,
    pub gateway_log: Option<String>,
    pub agent_level: String,
    pub agent_max_bytes: u64,
    pub agent_backup_count: u32,
    pub errors_max_bytes: u64,
    pub errors_backup_count: u32,
    pub gateway_component_prefixes: Vec<&'static str>,
}

pub fn builtin_skin_surfaces() -> &'static [SkinSurface] {
    &BUILTIN_SKIN_SURFACES
}

pub fn render_status(input: &CliStatusInput) -> String {
    let mut lines = vec![
        "Hermes CLI Status".to_string(),
        String::new(),
        format!("Session ID: {}", input.session_id),
        format!("Path: {}", input.display_path),
    ];
    if !input.title.trim().is_empty() {
        lines.push(format!("Title: {}", input.title.trim()));
    }
    lines.extend([
        format!("Model: {} ({})", input.model, input.provider),
        format!("Created: {}", input.created_at),
        format!("Last Activity: {}", input.last_activity),
        format!("Tokens: {}", format_u64_with_commas(input.total_tokens)),
        format!(
            "Agent Running: {}",
            if input.agent_running { "Yes" } else { "No" }
        ),
    ]);
    lines.join("\n")
}

pub fn logging_plan(
    hermes_home: &str,
    mode: Option<&str>,
    log_level: Option<&str>,
    max_size_mb: Option<u64>,
    backup_count: Option<u32>,
) -> LoggingPlan {
    let log_dir = format!("{}/logs", hermes_home.trim_end_matches('/'));
    let agent_level = log_level.unwrap_or("INFO").to_ascii_uppercase();
    let agent_max_bytes = max_size_mb.unwrap_or(5) * 1024 * 1024;
    let agent_backup_count = backup_count.unwrap_or(3);
    LoggingPlan {
        agent_log: format!("{}/agent.log", log_dir),
        errors_log: format!("{}/errors.log", log_dir),
        gateway_log: (mode == Some("gateway")).then(|| format!("{}/gateway.log", log_dir)),
        log_dir,
        agent_level,
        agent_max_bytes,
        agent_backup_count,
        errors_max_bytes: 2 * 1024 * 1024,
        errors_backup_count: 2,
        gateway_component_prefixes: vec!["gateway"],
    }
}

fn format_u64_with_commas(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (idx, ch) in raw.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

static BUILTIN_SKIN_SURFACES: [SkinSurface; 9] = [
    SkinSurface {
        name: "default",
        description: "Classic Hermes — gold and kawaii",
        tool_prefix: "┊",
        banner_title: "#FFD700",
        response_border: "#FFD700",
        status_bar_bg: "#1a1a2e",
        agent_name: "Hermes Agent",
        response_label: " ⚕ Hermes ",
        prompt_symbol: "❯",
        help_header: "(^_^)? Available Commands",
        spinner_wing_count: 0,
    },
    SkinSurface {
        name: "ares",
        description: "War-god theme — crimson and bronze",
        tool_prefix: "╎",
        banner_title: "#C7A96B",
        response_border: "#C7A96B",
        status_bar_bg: "#2A1212",
        agent_name: "Ares Agent",
        response_label: " ⚔ Ares ",
        prompt_symbol: "⚔",
        help_header: "(⚔) Available Commands",
        spinner_wing_count: 4,
    },
    SkinSurface {
        name: "mono",
        description: "Monochrome — clean grayscale",
        tool_prefix: "┊",
        banner_title: "#e6edf3",
        response_border: "#aaaaaa",
        status_bar_bg: "#1F1F1F",
        agent_name: "Hermes Agent",
        response_label: " ⚕ Hermes ",
        prompt_symbol: "❯",
        help_header: "[?] Available Commands",
        spinner_wing_count: 0,
    },
    SkinSurface {
        name: "slate",
        description: "Cool blue — developer-focused",
        tool_prefix: "┊",
        banner_title: "#7eb8f6",
        response_border: "#7eb8f6",
        status_bar_bg: "#151C2F",
        agent_name: "Hermes Agent",
        response_label: " ⚕ Hermes ",
        prompt_symbol: "❯",
        help_header: "(^_^)? Available Commands",
        spinner_wing_count: 0,
    },
    SkinSurface {
        name: "daylight",
        description: "Light theme for bright terminals with dark text and cool blue accents",
        tool_prefix: "│",
        banner_title: "#0F172A",
        response_border: "#2563EB",
        status_bar_bg: "#E5EDF8",
        agent_name: "Hermes Agent",
        response_label: " ⚕ Hermes ",
        prompt_symbol: "❯",
        help_header: "[?] Available Commands",
        spinner_wing_count: 0,
    },
    SkinSurface {
        name: "warm-lightmode",
        description: "Warm light mode — dark brown/gold text for light terminal backgrounds",
        tool_prefix: "┊",
        banner_title: "#5C3D11",
        response_border: "#8B6914",
        status_bar_bg: "#F5F0E8",
        agent_name: "Hermes Agent",
        response_label: " ⚕ Hermes ",
        prompt_symbol: "❯",
        help_header: "(^_^)? Available Commands",
        spinner_wing_count: 0,
    },
    SkinSurface {
        name: "poseidon",
        description: "Ocean-god theme — deep blue and seafoam",
        tool_prefix: "│",
        banner_title: "#A9DFFF",
        response_border: "#5DB8F5",
        status_bar_bg: "#0F2440",
        agent_name: "Poseidon Agent",
        response_label: " Ψ Poseidon ",
        prompt_symbol: "Ψ",
        help_header: "(Ψ) Available Commands",
        spinner_wing_count: 4,
    },
    SkinSurface {
        name: "sisyphus",
        description: "Sisyphean theme — austere grayscale with persistence",
        tool_prefix: "│",
        banner_title: "#F5F5F5",
        response_border: "#B7B7B7",
        status_bar_bg: "#202020",
        agent_name: "Sisyphus Agent",
        response_label: " ◉ Sisyphus ",
        prompt_symbol: "◉",
        help_header: "(◉) Available Commands",
        spinner_wing_count: 4,
    },
    SkinSurface {
        name: "charizard",
        description: "Volcanic theme — burnt orange and ember",
        tool_prefix: "│",
        banner_title: "#FFD39A",
        response_border: "#F29C38",
        status_bar_bg: "#2B160E",
        agent_name: "Charizard Agent",
        response_label: " ✦ Charizard ",
        prompt_symbol: "✦",
        help_header: "(✦) Available Commands",
        spinner_wing_count: 4,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_rendering_matches_cli_shape() {
        let status = render_status(&CliStatusInput {
            session_id: "session-123".to_string(),
            display_path: "~/.hermes".to_string(),
            title: "My titled session".to_string(),
            model: "openai/gpt-5.4".to_string(),
            provider: "openai".to_string(),
            created_at: "2026-04-09 19:24".to_string(),
            last_activity: "2026-04-09 19:25".to_string(),
            total_tokens: 1234,
            agent_running: false,
        });
        assert!(status.contains("Hermes CLI Status"));
        assert!(status.contains("Tokens: 1,234"));
        assert!(status.contains("Agent Running: No"));
    }

    #[test]
    fn gateway_logging_plan_adds_gateway_log() {
        let plan = logging_plan("/tmp/hermes", Some("gateway"), None, None, None);
        assert_eq!(plan.log_dir, "/tmp/hermes/logs");
        assert_eq!(plan.agent_log, "/tmp/hermes/logs/agent.log");
        assert_eq!(plan.errors_log, "/tmp/hermes/logs/errors.log");
        assert_eq!(
            plan.gateway_log.as_deref(),
            Some("/tmp/hermes/logs/gateway.log")
        );
        assert_eq!(plan.gateway_component_prefixes, vec!["gateway"]);
    }

    #[test]
    fn rendered_surfaces_do_not_use_erase_to_eol() {
        for skin in builtin_skin_surfaces() {
            let rendered = serde_json::to_string(skin).unwrap();
            assert!(!rendered.contains("\x1b[K"));
            assert!(!rendered.contains("\\033[K"));
        }
    }
}

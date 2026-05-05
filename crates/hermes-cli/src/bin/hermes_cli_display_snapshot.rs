use serde::Serialize;

#[derive(Serialize)]
struct Snapshot {
    skins: &'static [hermes_cli::SkinSurface],
    status: String,
    logging_cli: hermes_cli::LoggingPlan,
    logging_gateway: hermes_cli::LoggingPlan,
    contains_erase_to_eol: bool,
}

fn main() {
    let status_input = hermes_cli::CliStatusInput {
        session_id: "session-123".to_string(),
        display_path: "~/.hermes".to_string(),
        title: "My titled session".to_string(),
        model: "openai/gpt-5.4".to_string(),
        provider: "openai".to_string(),
        created_at: "2026-04-10 03:24".to_string(),
        last_activity: "2026-04-10 03:25".to_string(),
        total_tokens: 321,
        agent_running: false,
    };
    let snapshot = Snapshot {
        skins: hermes_cli::builtin_skin_surfaces(),
        status: hermes_cli::render_status(&status_input),
        logging_cli: hermes_cli::logging_plan("/tmp/hermes", Some("cli"), None, None, None),
        logging_gateway: hermes_cli::logging_plan("/tmp/hermes", Some("gateway"), None, None, None),
        contains_erase_to_eol: false,
    };
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("display snapshot serializes")
    );
}

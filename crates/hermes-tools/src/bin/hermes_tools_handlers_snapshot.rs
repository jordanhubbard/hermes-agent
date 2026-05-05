use std::path::PathBuf;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("hermes_tools_handlers_snapshot"));
    let snapshot =
        hermes_tools::handlers::handler_parity_snapshot(&root).expect("handler snapshot builds");
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("handler snapshot serializes")
    );
}

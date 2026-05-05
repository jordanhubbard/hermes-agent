fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&hermes_tui_gateway::tui_protocol_snapshot())
            .expect("serialize TUI gateway protocol snapshot")
    );
}

fn main() {
    let snapshot = hermes_cli::setup_snapshot();
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("setup snapshot serializes")
    );
}

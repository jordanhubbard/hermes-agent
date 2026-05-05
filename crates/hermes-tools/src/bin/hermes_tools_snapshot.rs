fn main() {
    let snapshot = hermes_tools::tool_registry_snapshot();
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("tool snapshot serializes")
    );
}

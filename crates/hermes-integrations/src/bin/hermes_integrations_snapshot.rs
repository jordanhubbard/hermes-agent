fn main() {
    let snapshot = hermes_integrations::integrations_snapshot();
    println!(
        "{}",
        serde_json::to_string_pretty(&snapshot).expect("integrations snapshot serializes")
    );
}

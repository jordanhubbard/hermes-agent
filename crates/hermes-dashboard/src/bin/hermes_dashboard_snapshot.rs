fn main() {
    let snapshot = hermes_dashboard::dashboard_snapshot();
    println!(
        "{}",
        serde_json::to_string_pretty(&snapshot).expect("dashboard snapshot serializes")
    );
}

use std::collections::BTreeSet;

fn main() {
    let config_gates: BTreeSet<String> = std::env::var("HERMES_CLI_CONFIG_GATES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    let snapshot = hermes_cli::registry_snapshot(&config_gates);
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("registry snapshot serializes")
    );
}

fn main() {
    let snapshot = hermes_tools::dispatch_parity_snapshot();
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("dispatch snapshot serializes")
    );
}

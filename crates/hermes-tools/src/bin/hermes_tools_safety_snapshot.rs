fn main() {
    let snapshot = hermes_tools::safety::safety_parity_snapshot();
    println!(
        "{}",
        serde_json::to_string(&snapshot).expect("safety snapshot serializes")
    );
}

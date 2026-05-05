fn main() {
    println!(
        "{}",
        serde_json::to_string_pretty(&hermes_acp::acp_parity_snapshot())
            .expect("serialize ACP parity snapshot")
    );
}

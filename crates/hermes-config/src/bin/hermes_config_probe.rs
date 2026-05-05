use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read probe JSON from stdin");
    let probe_input: hermes_config::ConfigProbeInput =
        serde_json::from_str(&input).expect("valid config probe JSON");
    let output = hermes_config::probe(probe_input);
    println!(
        "{}",
        serde_json::to_string(&output).expect("config probe serializes")
    );
}

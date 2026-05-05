//! Replay a backend-agnostic Hermes parity fixture.

use std::io::Read;
use std::path::PathBuf;

use hermes_agent_core::replay_fixture;
use serde_json::Value;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let input = match std::env::args_os().nth(1) {
        Some(path) if path != "-" => std::fs::read_to_string(PathBuf::from(path))?,
        _ => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };
    let fixture: Value = serde_json::from_str(&input)?;
    let result = replay_fixture(fixture)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

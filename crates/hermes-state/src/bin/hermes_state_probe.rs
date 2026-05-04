//! JSON probe for Python-side compatibility tests.
//!
//! This is intentionally small and stable: it lets Python tests exercise the
//! Rust `SessionStore` through a subprocess boundary. The actual op
//! dispatch lives in [`hermes_state::ops`] so the probe and the daemon
//! cannot drift.

use hermes_state::{ops::run_operation, SessionStore};
use serde_json::{json, Value};
use std::env;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err("usage: hermes_state_probe <schema-version|run-json> [db-path]".to_string());
    };

    match command.as_str() {
        "schema-version" => {
            let store = SessionStore::open_in_memory().map_err(|err| err.to_string())?;
            println!(
                "{}",
                json!({"schema_version": store.schema_version().map_err(|err| err.to_string())?})
            );
            Ok(())
        }
        "run-json" => {
            let Some(db_path) = args.next() else {
                return Err("usage: hermes_state_probe run-json <db-path>".to_string());
            };
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|err| err.to_string())?;
            let operations: Vec<Value> =
                serde_json::from_str(&input).map_err(|err| err.to_string())?;
            let mut store =
                SessionStore::open(PathBuf::from(db_path)).map_err(|err| err.to_string())?;
            let mut outputs = Vec::with_capacity(operations.len());
            for operation in operations {
                outputs.push(run_operation(&mut store, operation)?);
            }
            println!("{}", Value::Array(outputs));
            Ok(())
        }
        _ => Err(format!("unknown command: {command}")),
    }
}

use std::env;
use std::ffi::OsString;
use std::process;

use hermes_cli::launcher::{
    is_runtime_info_request, is_rust_help_request, is_rust_version_request, python_command,
    render_rust_help, render_rust_version, runtime_info, select_runtime, RuntimeSelection,
};

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let selection = match select_runtime(env::var("HERMES_RUNTIME").ok().as_deref()) {
        Ok(selection) => selection,
        Err(message) => {
            eprintln!("{message}");
            process::exit(64);
        }
    };

    if is_runtime_info_request(&args) {
        let info = runtime_info(selection, &args);
        println!(
            "{}",
            serde_json::to_string(&info).expect("runtime info serializes")
        );
        return;
    }

    let code = match selection {
        RuntimeSelection::Python => run_python(&args),
        RuntimeSelection::Rust => run_rust(&args),
    };
    process::exit(code);
}

fn run_python(args: &[OsString]) -> i32 {
    let mut command = match python_command(args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}");
            return 127;
        }
    };

    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(err) => {
            eprintln!("failed to start Python runtime: {err}");
            127
        }
    }
}

fn run_rust(args: &[OsString]) -> i32 {
    if is_rust_help_request(args) {
        print!("{}", render_rust_help());
        return 0;
    }

    if is_rust_version_request(args) {
        println!("{}", render_rust_version());
        return 0;
    }

    let command = args
        .first()
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_else(|| "chat".to_string());
    eprintln!(
        "HERMES_RUNTIME=rust selected, but command {command:?} is not Rust-owned yet. \
Use HERMES_RUNTIME=python for the rollout fallback. Full parity remains tracked by the hermes-fpr beads."
    );
    78
}

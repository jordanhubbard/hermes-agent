use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

pub const DEFAULT_RUNTIME: RuntimeSelection = RuntimeSelection::Python;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSelection {
    Rust,
    Python,
}

impl RuntimeSelection {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeSelection::Rust => "rust",
            RuntimeSelection::Python => "python",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeInfo {
    pub selected_runtime: RuntimeSelection,
    pub default_runtime: RuntimeSelection,
    pub selector_env: &'static str,
    pub args: Vec<String>,
}

pub fn select_runtime(value: Option<&str>) -> Result<RuntimeSelection, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_RUNTIME);
    };

    match value.to_ascii_lowercase().as_str() {
        "rust" => Ok(RuntimeSelection::Rust),
        "python" => Ok(RuntimeSelection::Python),
        "auto" | "default" => Ok(DEFAULT_RUNTIME),
        other => Err(format!(
            "invalid HERMES_RUNTIME={other:?}; expected 'rust', 'python', or 'auto'"
        )),
    }
}

pub fn runtime_info(selection: RuntimeSelection, args: &[OsString]) -> RuntimeInfo {
    RuntimeInfo {
        selected_runtime: selection,
        default_runtime: DEFAULT_RUNTIME,
        selector_env: "HERMES_RUNTIME",
        args: args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect(),
    }
}

pub fn is_runtime_info_request(args: &[OsString]) -> bool {
    args.iter()
        .any(|arg| arg == OsStr::new("--runtime-info") || arg == OsStr::new("runtime-info"))
}

pub fn is_rust_help_request(args: &[OsString]) -> bool {
    args.is_empty()
        || args.iter().any(|arg| {
            arg == OsStr::new("--help") || arg == OsStr::new("-h") || arg == OsStr::new("help")
        })
}

pub fn is_rust_version_request(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        arg == OsStr::new("--version") || arg == OsStr::new("-V") || arg == OsStr::new("version")
    })
}

pub fn is_rust_agent_runtime_smoke_request(args: &[OsString]) -> bool {
    args.first()
        .is_some_and(|arg| arg == OsStr::new("agent-runtime-smoke"))
}

pub fn is_rust_profile_status_request(args: &[OsString]) -> bool {
    args.len() == 1 && args.first().is_some_and(|arg| arg == OsStr::new("profile"))
}

pub fn render_rust_help() -> &'static str {
    "Hermes Agent Rust launcher\n\n\
Usage:\n  hermes [--runtime-info]\n  HERMES_RUNTIME=python hermes [args...]\n  HERMES_RUNTIME=rust hermes version\n  HERMES_RUNTIME=rust hermes agent-runtime-smoke\n\
  HERMES_RUNTIME=rust hermes profile\n\n\
Runtime selection:\n  HERMES_RUNTIME=python  Run the production Python runtime through hermes_cli.main\n  HERMES_RUNTIME=rust    Run Rust-owned commands that have landed so far\n  HERMES_RUNTIME=auto    Use the rollout default\n\n\
The Rust launcher owns process selection. Full Rust chat, gateway, TUI, dashboard,\n\
ACP, tools, skills, and plugin parity is tracked by the hermes-fpr beads.\n"
}

pub fn render_rust_version() -> String {
    format!("hermes rust launcher {}", env!("CARGO_PKG_VERSION"))
}

pub fn find_python_executable() -> Option<PathBuf> {
    if let Some(path) = env::var_os("HERMES_PYTHON").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }

    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            for candidate in python_candidates_under(ancestor) {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    Some(PathBuf::from("python3"))
}

fn python_candidates_under(root: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            root.join("venv").join("Scripts").join("python.exe"),
            root.join(".venv").join("Scripts").join("python.exe"),
        ]
    } else {
        vec![
            root.join("venv").join("bin").join("python"),
            root.join(".venv").join("bin").join("python"),
        ]
    }
}

pub fn python_command(args: &[OsString]) -> Result<Command, String> {
    let python = find_python_executable()
        .ok_or_else(|| "could not find Python executable for HERMES_RUNTIME=python".to_string())?;
    let mut command = Command::new(python);
    command.arg("-m").arg("hermes_cli.main").args(args);
    command.env("HERMES_RUST_LAUNCHER", "1");
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_selector_accepts_supported_values() {
        assert_eq!(select_runtime(None).unwrap(), RuntimeSelection::Python);
        assert_eq!(select_runtime(Some("")).unwrap(), RuntimeSelection::Python);
        assert_eq!(
            select_runtime(Some("auto")).unwrap(),
            RuntimeSelection::Python
        );
        assert_eq!(
            select_runtime(Some("default")).unwrap(),
            RuntimeSelection::Python
        );
        assert_eq!(
            select_runtime(Some("rust")).unwrap(),
            RuntimeSelection::Rust
        );
        assert_eq!(
            select_runtime(Some("PYTHON")).unwrap(),
            RuntimeSelection::Python
        );
        assert!(select_runtime(Some("node")).is_err());
    }

    #[test]
    fn runtime_info_preserves_args() {
        let args = vec![OsString::from("version"), OsString::from("--json")];
        let info = runtime_info(RuntimeSelection::Rust, &args);
        assert_eq!(info.selected_runtime, RuntimeSelection::Rust);
        assert_eq!(info.default_runtime, RuntimeSelection::Python);
        assert_eq!(info.args, vec!["version".to_string(), "--json".to_string()]);
    }

    #[test]
    fn rust_builtin_detection_is_intentional() {
        assert!(is_runtime_info_request(&[OsString::from("--runtime-info")]));
        assert!(is_rust_help_request(&[OsString::from("--help")]));
        assert!(is_rust_help_request(&[]));
        assert!(is_rust_version_request(&[OsString::from("version")]));
        assert!(is_rust_agent_runtime_smoke_request(&[OsString::from(
            "agent-runtime-smoke"
        )]));
        assert!(is_rust_profile_status_request(&[OsString::from("profile")]));
        assert!(!is_rust_profile_status_request(&[
            OsString::from("profile"),
            OsString::from("list")
        ]));
        assert!(!is_rust_version_request(&[OsString::from("gateway")]));
    }
}

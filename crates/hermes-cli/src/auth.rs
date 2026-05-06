use std::ffi::OsString;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthOutcome {
    pub output: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CredentialEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    auth_type: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    last_status: Option<String>,
    #[serde(default)]
    last_error_code: Option<i64>,
    #[serde(default)]
    last_error_reason: Option<String>,
    #[serde(default)]
    last_error_message: Option<String>,
}

pub fn run_auth_command(args: &[OsString], hermes_home: &Path) -> AuthOutcome {
    match args.get(1).map(|arg| arg.to_string_lossy()) {
        Some(action) if action == "list" => {
            let provider = args.get(2).map(|arg| normalize_provider(&arg.to_string_lossy()));
            AuthOutcome {
                output: render_auth_list(hermes_home, provider.as_deref()),
                exit_code: 0,
            }
        }
        Some(action) => AuthOutcome {
            output: format!(
                "HERMES_RUNTIME=rust selected, but auth action {action:?} is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n"
            ),
            exit_code: 78,
        },
        None => AuthOutcome {
            output: "HERMES_RUNTIME=rust selected, but interactive auth management is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n".to_string(),
            exit_code: 78,
        },
    }
}

fn render_auth_list(hermes_home: &Path, provider_filter: Option<&str>) -> String {
    let pool = read_credential_pool(hermes_home);
    let mut providers = if let Some(provider) = provider_filter.filter(|value| !value.is_empty()) {
        vec![provider.to_string()]
    } else {
        let mut keys = pool.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    };
    providers.dedup();

    let mut output = String::new();
    for provider in providers {
        let entries = pool
            .get(&provider)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value::<CredentialEntry>(item.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if entries.is_empty() {
            continue;
        }
        output.push_str(&format!("{provider} ({} credentials):\n", entries.len()));
        let current_id = entries
            .iter()
            .find(|entry| entry.last_status.as_deref() != Some("exhausted"))
            .map(|entry| entry.id.as_str());
        for (index, entry) in entries.iter().enumerate() {
            let marker = if current_id == Some(entry.id.as_str()) {
                "← "
            } else {
                "  "
            };
            let status = format_exhausted_status(entry);
            let line = format!(
                "  #{}  {:<20} {:<7} {}{} {}",
                index + 1,
                if entry.label.is_empty() {
                    &entry.source
                } else {
                    &entry.label
                },
                if entry.auth_type.is_empty() {
                    "api-key"
                } else {
                    &entry.auth_type
                },
                display_source(&entry.source),
                status,
                marker,
            );
            output.push_str(line.trim_end());
            output.push('\n');
        }
        output.push('\n');
    }
    output
}

fn read_credential_pool(hermes_home: &Path) -> serde_json::Map<String, Value> {
    let path = hermes_home.join("auth.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return serde_json::Map::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&raw) else {
        return serde_json::Map::new();
    };
    root.get("credential_pool")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn normalize_provider(provider: &str) -> String {
    let normalized = provider.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "or" | "open-router" => "openrouter".to_string(),
        _ => normalized,
    }
}

fn display_source(source: &str) -> &str {
    source.strip_prefix("manual:").unwrap_or(source)
}

fn format_exhausted_status(entry: &CredentialEntry) -> String {
    if entry.last_status.as_deref() != Some("exhausted") {
        return String::new();
    }
    let reason = entry
        .last_error_reason
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let message = entry
        .last_error_message
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let auth_failed = matches!(entry.last_error_code, Some(401 | 403))
        || [
            "invalid_token",
            "invalid_grant",
            "unauthorized",
            "forbidden",
            "auth",
        ]
        .iter()
        .any(|needle| reason.contains(needle))
        || [
            "unauthorized",
            "forbidden",
            "expired",
            "revoked",
            "invalid token",
            "authentication",
        ]
        .iter()
        .any(|needle| message.contains(needle));
    let label = if auth_failed {
        "auth failed"
    } else {
        "exhausted"
    };
    let reason_text = entry
        .last_error_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" {value}"))
        .unwrap_or_default();
    let code = entry
        .last_error_code
        .map(|code| format!(" ({code})"))
        .unwrap_or_default();
    if auth_failed {
        format!(" {label}{reason_text}{code} (re-auth may be required)")
    } else {
        format!(" {label}{reason_text}{code}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_manual_source_prefix() {
        assert_eq!(display_source("manual:device_code"), "device_code");
        assert_eq!(display_source("manual"), "manual");
    }
}

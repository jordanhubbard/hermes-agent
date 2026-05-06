use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use chrono::{Local, SecondsFormat};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthOutcome {
    pub output: String,
    pub error: String,
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
                error: String::new(),
                exit_code: 0,
            }
        }
        Some(action) if action == "remove" => {
            let Some(provider) = args.get(2).map(|arg| normalize_provider(&arg.to_string_lossy()))
            else {
                return AuthOutcome::usage("usage: hermes auth remove <provider> <target>\n");
            };
            let Some(target) = args.get(3).map(|arg| arg.to_string_lossy().into_owned()) else {
                return AuthOutcome::usage("usage: hermes auth remove <provider> <target>\n");
            };
            if args.len() > 4 {
                return AuthOutcome::usage("usage: hermes auth remove <provider> <target>\n");
            }
            remove_credential(hermes_home, &provider, &target)
        }
        Some(action) if action == "reset" => {
            let Some(provider) = args.get(2).map(|arg| normalize_provider(&arg.to_string_lossy()))
            else {
                return AuthOutcome::usage("usage: hermes auth reset <provider>\n");
            };
            if args.len() > 3 {
                return AuthOutcome::usage("usage: hermes auth reset <provider>\n");
            }
            reset_auth_statuses(hermes_home, &provider)
        }
        Some(action) => AuthOutcome::fallback(format!(
            "HERMES_RUNTIME=rust selected, but auth action {action:?} is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n"
        )),
        None => AuthOutcome::fallback(
            "HERMES_RUNTIME=rust selected, but interactive auth management is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n",
        ),
    }
}

impl AuthOutcome {
    fn ok(output: String) -> Self {
        Self {
            output,
            error: String::new(),
            exit_code: 0,
        }
    }

    fn usage(message: &str) -> Self {
        Self {
            output: String::new(),
            error: message.to_string(),
            exit_code: 2,
        }
    }

    fn failure(message: String) -> Self {
        Self {
            output: String::new(),
            error: message,
            exit_code: 1,
        }
    }

    fn fallback(message: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            error: message.into(),
            exit_code: 78,
        }
    }
}

fn remove_credential(hermes_home: &Path, provider: &str, target: &str) -> AuthOutcome {
    let path = hermes_home.join("auth.json");
    let mut root = read_auth_root(&path);

    let (removed_index, removed_label, removed_source, mut output) = {
        let entries = root
            .get_mut("credential_pool")
            .and_then(Value::as_object_mut)
            .and_then(|pool| pool.get_mut(provider))
            .and_then(Value::as_array_mut);
        let Some(entries) = entries else {
            let error = resolve_empty_target_error(target);
            return AuthOutcome::failure(format!("{error} Provider: {provider}.\n"));
        };

        let index = match resolve_credential_target(entries, provider, target) {
            Ok(index) => index,
            Err(error) => return AuthOutcome::failure(format!("{error} Provider: {provider}.\n")),
        };
        let removed = entries.remove(index);
        for (priority, entry) in entries.iter_mut().enumerate() {
            normalize_pooled_entry(entry, priority);
        }
        let label = credential_label(&removed, provider);
        let source = credential_source(&removed);
        (
            index + 1,
            label.clone(),
            source,
            format!("Removed {provider} credential #{} ({label})\n", index + 1),
        )
    };

    let mut cleanup_lines = Vec::new();
    apply_removal_cleanup(
        &mut root,
        hermes_home,
        provider,
        &removed_source,
        &mut cleanup_lines,
    );
    if !cleanup_lines.is_empty() {
        output.push_str(&cleanup_lines.join("\n"));
        output.push('\n');
    }
    ensure_auth_root(&mut root);
    match write_auth_root(&path, &root) {
        Ok(()) => AuthOutcome::ok(output),
        Err(err) => AuthOutcome::failure(format!(
            "Removed {provider} credential #{removed_index} ({removed_label}) in memory, but failed to write auth.json: {err}\n"
        )),
    }
}

fn read_auth_root(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

fn write_auth_root(path: &Path, root: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut raw = serde_json::to_string_pretty(root).map_err(std::io::Error::other)?;
    raw.push('\n');
    fs::write(path, raw)
}

fn ensure_auth_root(root: &mut Value) {
    if !root.is_object() {
        *root = Value::Object(serde_json::Map::new());
    }
    let Some(map) = root.as_object_mut() else {
        return;
    };
    map.entry("providers".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    map.entry("version".to_string())
        .or_insert_with(|| Value::Number(1.into()));
    map.insert("updated_at".to_string(), Value::String(now_iso()));
}

fn resolve_credential_target(
    entries: &[Value],
    provider: &str,
    target: &str,
) -> Result<usize, String> {
    let raw = target.trim();
    if raw.is_empty() {
        return Err("No credential target provided.".to_string());
    }

    for (index, entry) in entries.iter().enumerate() {
        if entry
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == raw)
        {
            return Ok(index);
        }
    }

    let raw_lower = raw.to_lowercase();
    let label_matches = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| credential_label(entry, provider).trim().to_lowercase() == raw_lower)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if label_matches.len() == 1 {
        return Ok(label_matches[0]);
    }
    if label_matches.len() > 1 {
        return Err(format!(
            "Ambiguous credential label \"{raw}\". Use the numeric index or entry id instead."
        ));
    }

    if raw.chars().all(|ch| ch.is_ascii_digit()) {
        let index = raw.parse::<usize>().unwrap_or(0);
        if (1..=entries.len()).contains(&index) {
            return Ok(index - 1);
        }
        return Err(format!("No credential #{index}."));
    }

    Err(format!("No credential matching \"{raw}\"."))
}

fn resolve_empty_target_error(target: &str) -> String {
    let raw = target.trim();
    if raw.is_empty() {
        "No credential target provided.".to_string()
    } else if raw.chars().all(|ch| ch.is_ascii_digit()) {
        format!("No credential #{}.", raw.parse::<usize>().unwrap_or(0))
    } else {
        format!("No credential matching \"{raw}\".")
    }
}

fn normalize_pooled_entry(entry: &mut Value, priority: usize) {
    let Some(map) = entry.as_object_mut() else {
        return;
    };
    map.insert("priority".to_string(), Value::Number(priority.into()));
    for key in [
        "last_status",
        "last_status_at",
        "last_error_code",
        "last_error_reason",
        "last_error_message",
        "last_error_reset_at",
    ] {
        map.entry(key.to_string()).or_insert(Value::Null);
    }
    map.entry("request_count".to_string())
        .or_insert_with(|| Value::Number(0.into()));
}

fn credential_label(entry: &Value, provider: &str) -> String {
    entry
        .get("label")
        .and_then(Value::as_str)
        .or_else(|| entry.get("source").and_then(Value::as_str))
        .unwrap_or(provider)
        .to_string()
}

fn credential_source(entry: &Value) -> String {
    entry
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("manual")
        .to_string()
}

fn apply_removal_cleanup(
    root: &mut Value,
    hermes_home: &Path,
    provider: &str,
    source: &str,
    lines: &mut Vec<String>,
) {
    if provider == "copilot" && (source == "gh_cli" || source.starts_with("env:")) {
        suppress_sources(
            root,
            provider,
            &[
                "gh_cli",
                "env:COPILOT_GITHUB_TOKEN",
                "env:GH_TOKEN",
                "env:GITHUB_TOKEN",
                source,
            ],
        );
        lines.push(
            "Suppressed all copilot token sources (gh_cli + env vars) — they will not be re-seeded."
                .to_string(),
        );
        lines.push("Note: Your gh CLI / shell environment is unchanged.".to_string());
        lines.push("Run `hermes auth add copilot` to re-enable if needed.".to_string());
        return;
    }

    if let Some(env_var) = source
        .strip_prefix("env:")
        .filter(|value| !value.is_empty())
    {
        remove_env_source(root, hermes_home, provider, source, env_var, lines);
        return;
    }

    match (provider, source) {
        ("anthropic", "claude_code") => {
            suppress_sources(root, provider, &[source]);
            lines.push("Suppressed claude_code credential — it will not be re-seeded.".to_string());
            lines.push(
                "Note: Claude Code credentials still live in ~/.claude/.credentials.json"
                    .to_string(),
            );
            lines.push("Run `hermes auth add anthropic` to re-enable if needed.".to_string());
        }
        ("anthropic", "hermes_pkce") => {
            let oauth_file = hermes_home.join(".anthropic_oauth.json");
            if oauth_file.exists() {
                match fs::remove_file(&oauth_file) {
                    Ok(()) => lines.push("Cleared Hermes Anthropic OAuth credentials".to_string()),
                    Err(err) => {
                        lines.push(format!("Could not delete {}: {err}", oauth_file.display()))
                    }
                }
            }
            suppress_sources(root, provider, &[source]);
        }
        ("nous", "device_code") => {
            if clear_provider_state(root, provider) {
                lines.push(format!("Cleared {provider} OAuth tokens from auth store"));
            }
            suppress_sources(root, provider, &[source]);
        }
        ("minimax-oauth", "oauth") => {
            if clear_provider_state(root, provider) {
                lines.push(format!("Cleared {provider} OAuth tokens from auth store"));
            }
            suppress_sources(root, provider, &[source]);
        }
        ("qwen-oauth", "qwen-cli") => {
            suppress_sources(root, provider, &[source]);
            lines.push("Suppressed qwen-cli credential — it will not be re-seeded.".to_string());
            lines.push(
                "Note: Qwen CLI credentials still live in ~/.qwen/oauth_creds.json".to_string(),
            );
            lines.push("Run `hermes auth add qwen-oauth` to re-enable if needed.".to_string());
        }
        _ if provider == "openai-codex"
            && (source == "device_code" || source.ends_with(":device_code")) =>
        {
            if clear_provider_state(root, provider) {
                lines.push(format!("Cleared {provider} OAuth tokens from auth store"));
            }
            suppress_sources(root, provider, &["device_code", source]);
            lines.push(
                "Suppressed openai-codex device_code source — it will not be re-seeded."
                    .to_string(),
            );
            lines.push("Note: Codex CLI credentials still live in ~/.codex/auth.json".to_string());
            lines.push("Run `hermes auth add openai-codex` to re-enable if needed.".to_string());
        }
        _ if source.starts_with("config:") || source == "model_config" => {
            suppress_sources(root, provider, &[source]);
            lines.push(format!("Suppressed {source} — it will not be re-seeded."));
            lines.push(
                "Note: The underlying value in config.yaml is unchanged.  Edit it directly if you want to remove the credential from disk."
                    .to_string(),
            );
        }
        _ => {}
    }
}

fn remove_env_source(
    root: &mut Value,
    hermes_home: &Path,
    provider: &str,
    source: &str,
    env_var: &str,
    lines: &mut Vec<String>,
) {
    let env_path = hermes_home.join(".env");
    let env_in_process = env::var_os(env_var).is_some();
    let env_in_dotenv = env_file_contains(&env_path, env_var);
    let shell_exported = env_in_process && !env_in_dotenv;
    if remove_env_line(&env_path, env_var).unwrap_or(false) {
        lines.push(format!("Cleared {env_var} from .env"));
    }
    suppress_sources(root, provider, &[source]);
    if shell_exported {
        lines.push(format!(
            "Note: {env_var} is still set in your shell environment (not in ~/.hermes/.env)."
        ));
        lines.push(
            "  Unset it there (shell profile, systemd EnvironmentFile, launchd plist, etc.) or it will keep being visible to Hermes."
                .to_string(),
        );
        lines.push(format!(
            "  The pool entry is now suppressed — Hermes will ignore {env_var} until you run `hermes auth add {provider}`."
        ));
    } else {
        lines.push(format!(
            "Suppressed env:{env_var} — it will not be re-seeded even if the variable is re-exported later."
        ));
    }
}

fn env_file_contains(path: &Path, env_var: &str) -> bool {
    fs::read_to_string(path).is_ok_and(|raw| {
        raw.lines()
            .any(|line| line.trim_start().starts_with(&format!("{env_var}=")))
    })
}

fn remove_env_line(path: &Path, env_var: &str) -> std::io::Result<bool> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    let prefix = format!("{env_var}=");
    let mut removed = false;
    let kept = raw
        .lines()
        .filter(|line| {
            let should_remove = line.trim_start().starts_with(&prefix);
            if should_remove {
                removed = true;
            }
            !should_remove
        })
        .collect::<Vec<_>>();
    if removed {
        let mut next = kept.join("\n");
        if !next.is_empty() {
            next.push('\n');
        }
        fs::write(path, next)?;
    }
    Ok(removed)
}

fn clear_provider_state(root: &mut Value, provider: &str) -> bool {
    root.get_mut("providers")
        .and_then(Value::as_object_mut)
        .and_then(|providers| providers.remove(provider))
        .is_some()
}

fn suppress_sources(root: &mut Value, provider: &str, sources: &[&str]) {
    if sources.is_empty() {
        return;
    }
    if !root.is_object() {
        *root = Value::Object(serde_json::Map::new());
    }
    let Some(map) = root.as_object_mut() else {
        return;
    };
    let suppressed = map
        .entry("suppressed_sources".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !suppressed.is_object() {
        *suppressed = Value::Object(serde_json::Map::new());
    }
    let Some(suppressed_map) = suppressed.as_object_mut() else {
        return;
    };
    let provider_list = suppressed_map
        .entry(provider.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !provider_list.is_array() {
        *provider_list = Value::Array(Vec::new());
    }
    let Some(provider_array) = provider_list.as_array_mut() else {
        return;
    };
    for source in sources.iter().copied().filter(|source| !source.is_empty()) {
        if !provider_array
            .iter()
            .any(|value| value.as_str() == Some(source))
        {
            provider_array.push(Value::String(source.to_string()));
        }
    }
}

fn reset_auth_statuses(hermes_home: &Path, provider: &str) -> AuthOutcome {
    let path = hermes_home.join("auth.json");
    let mut root = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    let mut count = 0;
    if let Some(entries) = root
        .get_mut("credential_pool")
        .and_then(Value::as_object_mut)
        .and_then(|pool| pool.get_mut(provider))
        .and_then(Value::as_array_mut)
    {
        for entry in entries {
            let Some(map) = entry.as_object_mut() else {
                continue;
            };
            let should_reset = ["last_status", "last_status_at", "last_error_code"]
                .iter()
                .any(|key| map.get(*key).is_some_and(|value| !value.is_null()));
            if should_reset {
                for key in [
                    "last_status",
                    "last_status_at",
                    "last_error_code",
                    "last_error_reason",
                    "last_error_message",
                    "last_error_reset_at",
                ] {
                    map.insert(key.to_string(), Value::Null);
                }
                map.entry("request_count".to_string())
                    .or_insert_with(|| Value::Number(0.into()));
                count += 1;
            }
        }
    }

    if count > 0 {
        if let Some(map) = root.as_object_mut() {
            map.entry("providers".to_string())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            map.entry("version".to_string())
                .or_insert_with(|| Value::Number(1.into()));
            map.insert("updated_at".to_string(), Value::String(now_iso()));
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(&root) {
            let _ = fs::write(path, raw);
        }
    }

    AuthOutcome::ok(format!("Reset status on {count} {provider} credentials\n"))
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

fn now_iso() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Micros, false)
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

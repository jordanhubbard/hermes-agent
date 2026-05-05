use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConfigProbeInput {
    pub home: String,
    #[serde(default)]
    pub hermes_home: Option<String>,
    #[serde(default)]
    pub default_config: Value,
    #[serde(default)]
    pub user_config: Value,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub current_dir: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PathSemantics {
    pub hermes_home: String,
    pub default_hermes_root: String,
    pub profiles_root: String,
    pub active_profile_path: String,
    pub display_hermes_home: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfigProbeOutput {
    pub paths: PathSemantics,
    pub loaded_config: Value,
    pub cli_env_bridge: BTreeMap<String, String>,
    pub gateway_env_bridge: BTreeMap<String, String>,
}

pub fn probe(input: ConfigProbeInput) -> ConfigProbeOutput {
    let home = PathBuf::from(&input.home);
    let hermes_home = input.hermes_home.as_deref().map(PathBuf::from);
    let loaded_config = load_config_from_values(
        input.default_config.clone(),
        input.user_config.clone(),
        &input.env,
    );
    ConfigProbeOutput {
        paths: path_semantics(&home, hermes_home.as_deref()),
        cli_env_bridge: bridge_cli_env(&loaded_config, &input.current_dir),
        gateway_env_bridge: bridge_gateway_env(&input.user_config, &input.env),
        loaded_config,
    }
}

pub fn path_semantics(home: &Path, hermes_home_env: Option<&Path>) -> PathSemantics {
    let hermes_home = get_hermes_home(home, hermes_home_env);
    let default_root = get_default_hermes_root(home, hermes_home_env);
    let profiles_root = default_root.join("profiles");
    let active_profile_path = default_root.join("active_profile");
    let display = display_hermes_home(home, &hermes_home);
    PathSemantics {
        hermes_home: path_string(&hermes_home),
        default_hermes_root: path_string(&default_root),
        profiles_root: path_string(&profiles_root),
        active_profile_path: path_string(&active_profile_path),
        display_hermes_home: display,
    }
}

pub fn get_hermes_home(home: &Path, hermes_home_env: Option<&Path>) -> PathBuf {
    hermes_home_env
        .filter(|value| !value.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".hermes"))
}

pub fn get_default_hermes_root(home: &Path, hermes_home_env: Option<&Path>) -> PathBuf {
    let native_home = home.join(".hermes");
    let Some(env_home) = hermes_home_env.filter(|value| !value.as_os_str().is_empty()) else {
        return native_home;
    };
    if path_starts_with(env_home, &native_home) {
        return native_home;
    }
    if env_home
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        == Some("profiles")
    {
        if let Some(root) = env_home.parent().and_then(Path::parent) {
            return root.to_path_buf();
        }
    }
    env_home.to_path_buf()
}

pub fn display_hermes_home(home: &Path, hermes_home: &Path) -> String {
    if let Ok(relative) = hermes_home.strip_prefix(home) {
        let rel = path_string(relative);
        if rel.is_empty() {
            "~/.".to_string()
        } else {
            format!("~/{}", rel)
        }
    } else {
        path_string(hermes_home)
    }
}

pub fn load_config_from_values(
    default_config: Value,
    user_config: Value,
    env: &BTreeMap<String, String>,
) -> Value {
    let mut config = default_config;
    let mut user_config = migrate_user_max_turns(user_config);
    if !user_config.is_object() {
        user_config = Value::Object(Map::new());
    }
    deep_merge_in_place(&mut config, &user_config);
    let normalized = normalize_root_model_keys(normalize_max_turns_config(config));
    expand_env_vars(&normalized, env)
}

pub fn deep_merge_in_place(base: &mut Value, override_value: &Value) {
    match (base, override_value) {
        (Value::Object(base_map), Value::Object(override_map)) => {
            for (key, value) in override_map {
                if let Some(base_value) = base_map.get_mut(key) {
                    if base_value.is_object() && value.is_object() {
                        deep_merge_in_place(base_value, value);
                    } else {
                        *base_value = value.clone();
                    }
                } else {
                    base_map.insert(key.clone(), value.clone());
                }
            }
        }
        (base_slot, value) => *base_slot = value.clone(),
    }
}

pub fn expand_env_vars(value: &Value, env: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(expand_env_string(text, env)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| expand_env_vars(item, env))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), expand_env_vars(value, env)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn bridge_cli_env(config: &Value, current_dir: &str) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let terminal = config.get("terminal").and_then(Value::as_object);
    let Some(terminal) = terminal else {
        return env;
    };
    let backend = terminal
        .get("env_type")
        .or_else(|| terminal.get("backend"))
        .and_then(value_to_string)
        .unwrap_or_else(|| "local".to_string());
    env.insert("TERMINAL_ENV".to_string(), backend.clone());
    if backend == "local" {
        env.insert("TERMINAL_CWD".to_string(), current_dir.to_string());
    } else if let Some(cwd) = terminal.get("cwd").and_then(value_to_string) {
        if !matches!(cwd.as_str(), "." | "auto" | "cwd") {
            env.insert("TERMINAL_CWD".to_string(), cwd);
        }
    }
    for (key, env_name) in TERMINAL_ENV_MAP {
        if *key == "backend" || *key == "env_type" || *key == "cwd" {
            continue;
        }
        if let Some(value) = terminal.get(*key).and_then(value_to_string) {
            env.insert((*env_name).to_string(), value);
        }
    }
    env
}

pub fn bridge_gateway_env(
    raw_config: &Value,
    env: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let expanded = expand_env_vars(raw_config, env);
    let mut out = BTreeMap::new();
    if let Some(root) = expanded.as_object() {
        for (key, value) in root {
            if is_scalar(value) && !env.contains_key(key) {
                if let Some(text) = value_to_string(value) {
                    out.insert(key.clone(), text);
                }
            }
        }
    }
    if let Some(terminal) = expanded.get("terminal").and_then(Value::as_object) {
        for (key, env_name) in GATEWAY_TERMINAL_ENV_MAP {
            if let Some(value) = terminal.get(*key) {
                if *key == "cwd" {
                    let Some(cwd) = value_to_string(value) else {
                        continue;
                    };
                    if matches!(cwd.as_str(), "." | "auto" | "cwd") {
                        continue;
                    }
                    out.insert((*env_name).to_string(), expand_tilde(&cwd, env));
                    continue;
                }
                if let Some(text) = value_to_string(value) {
                    out.insert((*env_name).to_string(), text);
                }
            }
        }
    }
    if let Some(agent) = expanded.get("agent").and_then(Value::as_object) {
        for (key, env_name) in AGENT_ENV_MAP {
            if let Some(value) = agent.get(*key).and_then(value_to_string) {
                out.insert((*env_name).to_string(), value);
            }
        }
    }
    out
}

const TERMINAL_ENV_MAP: &[(&str, &str)] = &[
    ("env_type", "TERMINAL_ENV"),
    ("backend", "TERMINAL_ENV"),
    ("cwd", "TERMINAL_CWD"),
    ("timeout", "TERMINAL_TIMEOUT"),
    ("lifetime_seconds", "TERMINAL_LIFETIME_SECONDS"),
    ("docker_image", "TERMINAL_DOCKER_IMAGE"),
    ("docker_forward_env", "TERMINAL_DOCKER_FORWARD_ENV"),
    ("singularity_image", "TERMINAL_SINGULARITY_IMAGE"),
    ("modal_image", "TERMINAL_MODAL_IMAGE"),
    ("daytona_image", "TERMINAL_DAYTONA_IMAGE"),
    ("vercel_runtime", "TERMINAL_VERCEL_RUNTIME"),
    ("ssh_host", "TERMINAL_SSH_HOST"),
    ("ssh_user", "TERMINAL_SSH_USER"),
    ("ssh_port", "TERMINAL_SSH_PORT"),
    ("ssh_key", "TERMINAL_SSH_KEY"),
    ("container_cpu", "TERMINAL_CONTAINER_CPU"),
    ("container_memory", "TERMINAL_CONTAINER_MEMORY"),
    ("container_disk", "TERMINAL_CONTAINER_DISK"),
    ("container_persistent", "TERMINAL_CONTAINER_PERSISTENT"),
    ("docker_volumes", "TERMINAL_DOCKER_VOLUMES"),
    (
        "docker_mount_cwd_to_workspace",
        "TERMINAL_DOCKER_MOUNT_CWD_TO_WORKSPACE",
    ),
    (
        "docker_run_as_host_user",
        "TERMINAL_DOCKER_RUN_AS_HOST_USER",
    ),
    ("sandbox_dir", "TERMINAL_SANDBOX_DIR"),
    ("persistent_shell", "TERMINAL_PERSISTENT_SHELL"),
    ("sudo_password", "SUDO_PASSWORD"),
];

const GATEWAY_TERMINAL_ENV_MAP: &[(&str, &str)] = &[
    ("backend", "TERMINAL_ENV"),
    ("cwd", "TERMINAL_CWD"),
    ("timeout", "TERMINAL_TIMEOUT"),
    ("lifetime_seconds", "TERMINAL_LIFETIME_SECONDS"),
    ("docker_image", "TERMINAL_DOCKER_IMAGE"),
    ("docker_forward_env", "TERMINAL_DOCKER_FORWARD_ENV"),
    ("singularity_image", "TERMINAL_SINGULARITY_IMAGE"),
    ("modal_image", "TERMINAL_MODAL_IMAGE"),
    ("daytona_image", "TERMINAL_DAYTONA_IMAGE"),
    ("vercel_runtime", "TERMINAL_VERCEL_RUNTIME"),
    ("ssh_host", "TERMINAL_SSH_HOST"),
    ("ssh_user", "TERMINAL_SSH_USER"),
    ("ssh_port", "TERMINAL_SSH_PORT"),
    ("ssh_key", "TERMINAL_SSH_KEY"),
    ("container_cpu", "TERMINAL_CONTAINER_CPU"),
    ("container_memory", "TERMINAL_CONTAINER_MEMORY"),
    ("container_disk", "TERMINAL_CONTAINER_DISK"),
    ("container_persistent", "TERMINAL_CONTAINER_PERSISTENT"),
    ("docker_volumes", "TERMINAL_DOCKER_VOLUMES"),
    (
        "docker_mount_cwd_to_workspace",
        "TERMINAL_DOCKER_MOUNT_CWD_TO_WORKSPACE",
    ),
    (
        "docker_run_as_host_user",
        "TERMINAL_DOCKER_RUN_AS_HOST_USER",
    ),
    ("sandbox_dir", "TERMINAL_SANDBOX_DIR"),
    ("persistent_shell", "TERMINAL_PERSISTENT_SHELL"),
];

const AGENT_ENV_MAP: &[(&str, &str)] = &[
    ("max_turns", "HERMES_MAX_ITERATIONS"),
    ("gateway_timeout", "HERMES_AGENT_TIMEOUT"),
    ("gateway_timeout_warning", "HERMES_AGENT_TIMEOUT_WARNING"),
    ("gateway_notify_interval", "HERMES_AGENT_NOTIFY_INTERVAL"),
    ("restart_drain_timeout", "HERMES_RESTART_DRAIN_TIMEOUT"),
    (
        "gateway_auto_continue_freshness",
        "HERMES_AUTO_CONTINUE_FRESHNESS",
    ),
];

fn migrate_user_max_turns(mut user_config: Value) -> Value {
    let Some(root) = user_config.as_object_mut() else {
        return Value::Object(Map::new());
    };
    let Some(max_turns) = root.get("max_turns").cloned() else {
        return user_config;
    };
    let mut agent = root
        .get("agent")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if agent.get("max_turns").is_none_or(Value::is_null) {
        agent.insert("max_turns".to_string(), max_turns);
    }
    root.insert("agent".to_string(), Value::Object(agent));
    root.remove("max_turns");
    user_config
}

fn normalize_max_turns_config(mut config: Value) -> Value {
    let Some(root) = config.as_object_mut() else {
        return config;
    };
    let root_max_turns = root.get("max_turns").cloned();
    let default_max_turns = root
        .get("agent")
        .and_then(Value::as_object)
        .and_then(|agent| agent.get("max_turns"))
        .cloned()
        .unwrap_or(Value::from(90));
    let mut agent = root
        .get("agent")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(max_turns) = root_max_turns {
        agent.entry("max_turns".to_string()).or_insert(max_turns);
    }
    agent
        .entry("max_turns".to_string())
        .or_insert(default_max_turns);
    root.insert("agent".to_string(), Value::Object(agent));
    root.remove("max_turns");
    config
}

fn normalize_root_model_keys(mut config: Value) -> Value {
    let Some(root) = config.as_object_mut() else {
        return config;
    };
    if !["provider", "base_url", "context_length"]
        .iter()
        .any(|key| root.get(*key).is_some_and(is_truthy))
    {
        return config;
    }
    let mut model = match root.get("model") {
        Some(Value::Object(map)) => map.clone(),
        Some(value) if is_truthy(value) => {
            let mut map = Map::new();
            map.insert("default".to_string(), value.clone());
            map
        }
        _ => Map::new(),
    };
    for key in ["provider", "base_url", "context_length"] {
        if let Some(value) = root.get(key).filter(|value| is_truthy(value)).cloned() {
            if model.get(key).is_none_or(|existing| !is_truthy(existing)) {
                model.insert(key.to_string(), value);
            }
        }
        root.remove(key);
    }
    root.insert("model".to_string(), Value::Object(model));
    config
}

fn expand_env_string(text: &str, env: &BTreeMap<String, String>) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let key = &after_start[..end];
        if let Some(value) = env.get(key) {
            output.push_str(value);
        } else {
            output.push_str("${");
            output.push_str(key);
            output.push('}');
        }
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn path_starts_with(path: &Path, base: &Path) -> bool {
    normalize_components(path).starts_with(&normalize_components(base))
}

fn normalize_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::RootDir => Some("/".to_string()),
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            Component::Prefix(value) => Some(value.as_os_str().to_string_lossy().to_string()),
            Component::CurDir => None,
            Component::ParentDir => Some("..".to_string()),
        })
        .collect()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
    )
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(if *value { "True" } else { "False" }.to_string()),
        Value::Array(_) | Value::Object(_) => Some(python_json_string(value)),
        Value::Null => None,
    }
}

fn python_json_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(number) => number.to_string(),
        Value::String(text) => serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string()),
        Value::Array(items) => {
            let rendered: Vec<String> = items.iter().map(python_json_string).collect();
            format!("[{}]", rendered.join(", "))
        }
        Value::Object(map) => {
            let rendered: Vec<String> = map
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        serde_json::to_string(key).unwrap_or_else(|_| "\"\"".to_string()),
                        python_json_string(value)
                    )
                })
                .collect();
            format!("{{{}}}", rendered.join(", "))
        }
    }
}

fn expand_tilde(value: &str, env: &BTreeMap<String, String>) -> String {
    if value == "~" {
        env.get("HOME")
            .cloned()
            .unwrap_or_else(|| value.to_string())
    } else if let Some(rest) = value.strip_prefix("~/") {
        env.get("HOME")
            .map(|home| format!("{}/{}", home.trim_end_matches('/'), rest))
            .unwrap_or_else(|| value.to_string())
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profile_paths_match_standard_and_custom_layouts() {
        let home = Path::new("/tmp/home");
        let default = path_semantics(home, None);
        assert_eq!(default.hermes_home, "/tmp/home/.hermes");
        assert_eq!(default.display_hermes_home, "~/.hermes");

        let profile = path_semantics(home, Some(Path::new("/tmp/home/.hermes/profiles/coder")));
        assert_eq!(profile.default_hermes_root, "/tmp/home/.hermes");
        assert_eq!(profile.profiles_root, "/tmp/home/.hermes/profiles");
        assert_eq!(profile.display_hermes_home, "~/.hermes/profiles/coder");

        let docker_profile = path_semantics(home, Some(Path::new("/opt/data/profiles/coder")));
        assert_eq!(docker_profile.default_hermes_root, "/opt/data");
    }

    #[test]
    fn config_merge_migrates_legacy_keys_and_expands_env_refs() {
        let default = json!({
            "model": "",
            "agent": {"max_turns": 90, "gateway_timeout": 1800},
            "terminal": {"backend": "local", "cwd": ".", "timeout": 180}
        });
        let user = json!({
            "max_turns": 12,
            "provider": "openai",
            "base_url": "${BASE_URL}",
            "terminal": {"timeout": 30}
        });
        let env = BTreeMap::from([("BASE_URL".to_string(), "https://api.example".to_string())]);
        let loaded = load_config_from_values(default, user, &env);
        assert_eq!(loaded["agent"]["max_turns"], 12);
        assert_eq!(loaded["model"]["provider"], "openai");
        assert_eq!(loaded["model"]["base_url"], "https://api.example");
        assert_eq!(loaded["terminal"]["backend"], "local");
        assert_eq!(loaded["terminal"]["timeout"], 30);
    }

    #[test]
    fn gateway_bridge_skips_cwd_placeholders_and_maps_agent_env() {
        let raw = json!({
            "terminal": {"backend": "docker", "cwd": ".", "timeout": 44},
            "agent": {"max_turns": 22, "gateway_timeout": 99}
        });
        let bridged = bridge_gateway_env(&raw, &BTreeMap::new());
        assert_eq!(bridged.get("TERMINAL_ENV").unwrap(), "docker");
        assert_eq!(bridged.get("TERMINAL_TIMEOUT").unwrap(), "44");
        assert!(!bridged.contains_key("TERMINAL_CWD"));
        assert_eq!(bridged.get("HERMES_MAX_ITERATIONS").unwrap(), "22");
        assert_eq!(bridged.get("HERMES_AGENT_TIMEOUT").unwrap(), "99");
    }
}

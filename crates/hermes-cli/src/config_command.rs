use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use serde_yaml::{Mapping, Number, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigCommandOutcome {
    pub output: String,
    pub exit_code: i32,
}

pub fn run_config_set_command(args: &[OsString], hermes_home: &Path) -> ConfigCommandOutcome {
    let Some(key) = args.get(2).map(|arg| arg.to_string_lossy().into_owned()) else {
        return ConfigCommandOutcome {
            output: usage().to_string(),
            exit_code: 1,
        };
    };
    let Some(raw_value) = args.get(3).map(|arg| arg.to_string_lossy().into_owned()) else {
        return ConfigCommandOutcome {
            output: usage().to_string(),
            exit_code: 1,
        };
    };
    if args.len() > 4 {
        return ConfigCommandOutcome {
            output: usage().to_string(),
            exit_code: 1,
        };
    }

    if is_env_key(&key) {
        let env_path = hermes_home.join(".env");
        if let Err(message) = save_env_value(&env_path, &key.to_ascii_uppercase(), &raw_value) {
            return ConfigCommandOutcome {
                output: format!("Error: {message}\n"),
                exit_code: 1,
            };
        }
        return ConfigCommandOutcome {
            output: format!("✓ Set {key} in {}\n", env_path.display()),
            exit_code: 0,
        };
    }

    let value = parse_config_value(&raw_value);
    let display_value = display_config_value(&value);
    let config_path = hermes_home.join("config.yaml");
    let mut config = read_yaml_mapping(&config_path);
    if let Err(message) = set_nested(&mut config, &key, value.clone()) {
        return ConfigCommandOutcome {
            output: format!("Error: {message}\n"),
            exit_code: 1,
        };
    }
    if let Err(message) = write_yaml_mapping(&config_path, &config) {
        return ConfigCommandOutcome {
            output: format!("Error: {message}\n"),
            exit_code: 1,
        };
    }
    if let Some(env_name) = config_env_sync_name(&key) {
        let env_path = hermes_home.join(".env");
        if let Err(message) = save_env_value(&env_path, env_name, &display_value) {
            return ConfigCommandOutcome {
                output: format!("Error: {message}\n"),
                exit_code: 1,
            };
        }
    }
    ConfigCommandOutcome {
        output: format!(
            "✓ Set {key} = {display_value} in {}\n",
            config_path.display()
        ),
        exit_code: 0,
    }
}

pub fn is_config_set_args(args: &[OsString]) -> bool {
    args.first().is_some_and(|arg| arg == OsStr::new("config"))
        && args.get(1).is_some_and(|arg| arg == OsStr::new("set"))
}

fn usage() -> &'static str {
    "Usage: hermes config set <key> <value>\n\n\
Examples:\n  hermes config set model anthropic/claude-sonnet-4\n  hermes config set terminal.backend docker\n  hermes config set OPENROUTER_API_KEY sk-or-...\n"
}

fn is_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    API_KEYS.contains(&upper.as_str())
        || upper.ends_with("_API_KEY")
        || upper.ends_with("_TOKEN")
        || upper.starts_with("TERMINAL_SSH")
}

fn parse_config_value(value: &str) -> Value {
    let lower = value.to_ascii_lowercase();
    if matches!(lower.as_str(), "true" | "yes" | "on") {
        return Value::Bool(true);
    }
    if matches!(lower.as_str(), "false" | "no" | "off") {
        return Value::Bool(false);
    }
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        if let Ok(parsed) = value.parse::<i64>() {
            return Value::Number(Number::from(parsed));
        }
    }
    if value.chars().filter(|ch| *ch == '.').count() == 1
        && value.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
    {
        if let Ok(parsed) = value.parse::<f64>() {
            if let Ok(yaml_value) = serde_yaml::to_value(parsed) {
                return yaml_value;
            }
        }
    }
    Value::String(value.to_string())
}

fn display_config_value(value: &Value) -> String {
    match value {
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn read_yaml_mapping(path: &Path) -> Mapping {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_yaml::from_str::<Value>(&content).ok())
        .and_then(|value| value.as_mapping().cloned())
        .unwrap_or_default()
}

fn write_yaml_mapping(path: &Path, mapping: &Mapping) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let content = serde_yaml::to_string(mapping).map_err(|err| err.to_string())?;
    fs::write(path, content).map_err(|err| err.to_string())
}

fn set_nested(mapping: &mut Mapping, dotted_key: &str, value: Value) -> Result<(), String> {
    let parts = dotted_key.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(format!("invalid config key {dotted_key:?}"));
    }
    set_nested_value(&mut Value::Mapping(mapping.clone()), &parts, value).and_then(|updated| {
        if let Value::Mapping(updated_mapping) = updated {
            *mapping = updated_mapping;
            Ok(())
        } else {
            Err("config root is not a mapping".to_string())
        }
    })
}

fn set_nested_value(current: &mut Value, parts: &[&str], value: Value) -> Result<Value, String> {
    if parts.len() == 1 {
        match current {
            Value::Mapping(mapping) => {
                mapping.insert(Value::String(parts[0].to_string()), value);
                return Ok(current.clone());
            }
            Value::Sequence(items) => {
                let idx = parts[0]
                    .parse::<usize>()
                    .map_err(|_| format!("segment {:?} is not a numeric index", parts[0]))?;
                let slot = items
                    .get_mut(idx)
                    .ok_or_else(|| format!("list index {idx} out of range"))?;
                *slot = value;
                return Ok(current.clone());
            }
            _ => return Err("cannot set nested value on scalar".to_string()),
        }
    }

    match current {
        Value::Mapping(mapping) => {
            let key = Value::String(parts[0].to_string());
            let entry = mapping
                .entry(key)
                .or_insert_with(|| Value::Mapping(Mapping::new()));
            if !matches!(entry, Value::Mapping(_) | Value::Sequence(_)) {
                *entry = Value::Mapping(Mapping::new());
            }
            let updated = set_nested_value(entry, &parts[1..], value)?;
            *entry = updated;
            Ok(current.clone())
        }
        Value::Sequence(items) => {
            let idx = parts[0]
                .parse::<usize>()
                .map_err(|_| format!("segment {:?} is not a numeric index", parts[0]))?;
            let slot = items
                .get_mut(idx)
                .ok_or_else(|| format!("list index {idx} out of range"))?;
            let updated = set_nested_value(slot, &parts[1..], value)?;
            *slot = updated;
            Ok(current.clone())
        }
        _ => Err("cannot navigate into scalar config value".to_string()),
    }
}

fn save_env_value(path: &Path, key: &str, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut lines = fs::read_to_string(path)
        .ok()
        .map(|content| content.lines().map(str::to_string).collect::<Vec<_>>())
        .unwrap_or_default();
    let prefix = format!("{key}=");
    let replacement = format!("{key}={value}");
    let mut replaced = false;
    for line in &mut lines {
        if line.starts_with(&prefix) {
            *line = replacement.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.push(replacement);
    }
    fs::write(path, format!("{}\n", lines.join("\n"))).map_err(|err| err.to_string())
}

fn config_env_sync_name(key: &str) -> Option<&'static str> {
    CONFIG_TO_ENV_SYNC
        .iter()
        .find_map(|(config_key, env_name)| (*config_key == key).then_some(*env_name))
}

const API_KEYS: &[&str] = &[
    "OPENROUTER_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "VOICE_TOOLS_OPENAI_KEY",
    "EXA_API_KEY",
    "PARALLEL_API_KEY",
    "FIRECRAWL_API_KEY",
    "FIRECRAWL_API_URL",
    "FIRECRAWL_GATEWAY_URL",
    "TOOL_GATEWAY_DOMAIN",
    "TOOL_GATEWAY_SCHEME",
    "TOOL_GATEWAY_USER_TOKEN",
    "TAVILY_API_KEY",
    "BROWSERBASE_API_KEY",
    "BROWSERBASE_PROJECT_ID",
    "BROWSER_USE_API_KEY",
    "FAL_KEY",
    "TELEGRAM_BOT_TOKEN",
    "DISCORD_BOT_TOKEN",
    "TERMINAL_SSH_HOST",
    "TERMINAL_SSH_USER",
    "TERMINAL_SSH_KEY",
    "SUDO_PASSWORD",
    "SLACK_BOT_TOKEN",
    "SLACK_APP_TOKEN",
    "GITHUB_TOKEN",
    "HONCHO_API_KEY",
    "WANDB_API_KEY",
    "TINKER_API_KEY",
];

const CONFIG_TO_ENV_SYNC: &[(&str, &str)] = &[
    ("terminal.backend", "TERMINAL_ENV"),
    ("terminal.modal_mode", "TERMINAL_MODAL_MODE"),
    ("terminal.docker_image", "TERMINAL_DOCKER_IMAGE"),
    ("terminal.singularity_image", "TERMINAL_SINGULARITY_IMAGE"),
    ("terminal.modal_image", "TERMINAL_MODAL_IMAGE"),
    ("terminal.daytona_image", "TERMINAL_DAYTONA_IMAGE"),
    ("terminal.vercel_runtime", "TERMINAL_VERCEL_RUNTIME"),
    (
        "terminal.docker_mount_cwd_to_workspace",
        "TERMINAL_DOCKER_MOUNT_CWD_TO_WORKSPACE",
    ),
    (
        "terminal.docker_run_as_host_user",
        "TERMINAL_DOCKER_RUN_AS_HOST_USER",
    ),
    ("terminal.timeout", "TERMINAL_TIMEOUT"),
    ("terminal.sandbox_dir", "TERMINAL_SANDBOX_DIR"),
    ("terminal.persistent_shell", "TERMINAL_PERSISTENT_SHELL"),
    ("terminal.container_cpu", "TERMINAL_CONTAINER_CPU"),
    ("terminal.container_memory", "TERMINAL_CONTAINER_MEMORY"),
    ("terminal.container_disk", "TERMINAL_CONTAINER_DISK"),
    (
        "terminal.container_persistent",
        "TERMINAL_CONTAINER_PERSISTENT",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_python_style_values() {
        assert_eq!(display_config_value(&parse_config_value("true")), "True");
        assert_eq!(display_config_value(&parse_config_value("123")), "123");
        assert_eq!(display_config_value(&parse_config_value("1.5")), "1.5");
        assert_eq!(display_config_value(&parse_config_value("model")), "model");
    }
}

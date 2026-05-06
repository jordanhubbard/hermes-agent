use std::env;
use std::ffi::OsString;
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

pub fn run_config_show_command(args: &[OsString], hermes_home: &Path) -> ConfigCommandOutcome {
    if args.len() > 2 {
        return ConfigCommandOutcome {
            output: "Usage: hermes config show\n".to_string(),
            exit_code: 1,
        };
    }

    let (config, warning) = load_display_config(&hermes_home.join("config.yaml"));
    let env_path = hermes_home.join(".env");
    let env_file = read_env_file(&env_path);
    let mut output = String::new();

    if let Some(warning) = warning {
        output.push_str(&format!("Warning: Failed to load config: {warning}\n"));
    }

    output.push('\n');
    output.push_str("┌─────────────────────────────────────────────────────────┐\n");
    output.push_str("│              ⚕ Hermes Configuration                    │\n");
    output.push_str("└─────────────────────────────────────────────────────────┘\n");

    output.push_str("\n◆ Paths\n");
    output.push_str(&format!(
        "  Config:       {}\n",
        hermes_home.join("config.yaml").display()
    ));
    output.push_str(&format!("  Secrets:      {}\n", env_path.display()));
    output.push_str(&format!("  Install:      {}\n", project_root().display()));

    output.push_str("\n◆ API Keys\n");
    for (env_key, name) in API_KEY_DISPLAY {
        output.push_str(&format!(
            "  {name:<14} {}\n",
            redact_key(env_value(env_key, &env_file).as_deref())
        ));
    }
    output.push_str(&format!(
        "  {:<14} {}\n",
        "Anthropic",
        redact_key(env_value("ANTHROPIC_API_KEY", &env_file).as_deref())
    ));

    output.push_str("\n◆ Model\n");
    let model_display = config_get(&config, &["model"])
        .map(display_show_value)
        .unwrap_or_else(|| "not set".to_string());
    output.push_str(&format!("  Model:        {}\n", model_display));
    output.push_str(&format!(
        "  Max turns:    {}\n",
        scalar_or_default(&config, &["agent", "max_turns"], "90")
    ));

    output.push_str("\n◆ Display\n");
    output.push_str(&format!(
        "  Personality:  {}\n",
        scalar_or_default(&config, &["display", "personality"], "kawaii")
    ));
    output.push_str(&format!(
        "  Reasoning:    {}\n",
        if bool_or_default(&config, &["display", "show_reasoning"], false) {
            "on"
        } else {
            "off"
        }
    ));
    output.push_str(&format!(
        "  Bell:         {}\n",
        if bool_or_default(&config, &["display", "bell_on_complete"], false) {
            "on"
        } else {
            "off"
        }
    ));
    output.push_str(&format!(
        "  User preview: first {} line(s), last {} line(s)\n",
        scalar_or_default(
            &config,
            &["display", "user_message_preview", "first_lines"],
            "2"
        ),
        scalar_or_default(
            &config,
            &["display", "user_message_preview", "last_lines"],
            "2"
        )
    ));

    output.push_str("\n◆ Terminal\n");
    let backend = scalar_or_default(&config, &["terminal", "backend"], "local");
    output.push_str(&format!("  Backend:      {backend}\n"));
    output.push_str(&format!(
        "  Working dir:  {}\n",
        scalar_or_default(&config, &["terminal", "cwd"], ".")
    ));
    output.push_str(&format!(
        "  Timeout:      {}s\n",
        scalar_or_default(&config, &["terminal", "timeout"], "180")
    ));
    match backend.as_str() {
        "docker" => output.push_str(&format!(
            "  Docker image: {}\n",
            scalar_or_default(
                &config,
                &["terminal", "docker_image"],
                "nikolaik/python-nodejs:python3.11-nodejs20"
            )
        )),
        "singularity" => output.push_str(&format!(
            "  Image:        {}\n",
            scalar_or_default(
                &config,
                &["terminal", "singularity_image"],
                "docker://nikolaik/python-nodejs:python3.11-nodejs20"
            )
        )),
        "modal" => {
            output.push_str(&format!(
                "  Modal image:  {}\n",
                scalar_or_default(
                    &config,
                    &["terminal", "modal_image"],
                    "nikolaik/python-nodejs:python3.11-nodejs20"
                )
            ));
            output.push_str(&format!(
                "  Modal token:  {}\n",
                if has_env_value("MODAL_TOKEN_ID", &env_file) {
                    "configured"
                } else {
                    "(not set)"
                }
            ));
        }
        "daytona" => {
            output.push_str(&format!(
                "  Daytona image: {}\n",
                scalar_or_default(
                    &config,
                    &["terminal", "daytona_image"],
                    "nikolaik/python-nodejs:python3.11-nodejs20"
                )
            ));
            output.push_str(&format!(
                "  API key:      {}\n",
                if has_env_value("DAYTONA_API_KEY", &env_file) {
                    "configured"
                } else {
                    "(not set)"
                }
            ));
        }
        "vercel_sandbox" => {
            output.push_str(&format!(
                "  Vercel runtime: {}\n",
                scalar_or_default(&config, &["terminal", "vercel_runtime"], "node24")
            ));
            let has_vercel_auth = has_env_value("VERCEL_OIDC_TOKEN", &env_file)
                || (has_env_value("VERCEL_TOKEN", &env_file)
                    && has_env_value("VERCEL_PROJECT_ID", &env_file)
                    && has_env_value("VERCEL_TEAM_ID", &env_file));
            output.push_str(&format!(
                "  Vercel auth:    {}\n",
                if has_vercel_auth {
                    "configured"
                } else {
                    "(not set)"
                }
            ));
        }
        "ssh" => {
            output.push_str(&format!(
                "  SSH host:     {}\n",
                display_env_value("TERMINAL_SSH_HOST", &env_file)
            ));
            output.push_str(&format!(
                "  SSH user:     {}\n",
                display_env_value("TERMINAL_SSH_USER", &env_file)
            ));
        }
        _ => {}
    }

    output.push_str("\n◆ Timezone\n");
    let timezone = scalar_or_default(&config, &["timezone"], "");
    if timezone.is_empty() {
        output.push_str("  Timezone:     (server-local)\n");
    } else {
        output.push_str(&format!("  Timezone:     {timezone}\n"));
    }

    output.push_str("\n◆ Context Compression\n");
    let compression_enabled = bool_or_default(&config, &["compression", "enabled"], true);
    output.push_str(&format!(
        "  Enabled:      {}\n",
        if compression_enabled { "yes" } else { "no" }
    ));
    if compression_enabled {
        output.push_str(&format!(
            "  Threshold:    {:.0}%\n",
            float_or_default(&config, &["compression", "threshold"], 0.50) * 100.0
        ));
        output.push_str(&format!(
            "  Target ratio: {:.0}% of threshold preserved\n",
            float_or_default(&config, &["compression", "target_ratio"], 0.20) * 100.0
        ));
        output.push_str(&format!(
            "  Protect last: {} messages\n",
            scalar_or_default(&config, &["compression", "protect_last_n"], "20")
        ));
        let compression_model =
            scalar_or_default(&config, &["auxiliary", "compression", "model"], "");
        output.push_str(&format!(
            "  Model:        {}\n",
            if compression_model.is_empty() {
                "(auto)"
            } else {
                compression_model.as_str()
            }
        ));
        let compression_provider =
            scalar_or_default(&config, &["auxiliary", "compression", "provider"], "auto");
        if !compression_provider.is_empty() && compression_provider != "auto" {
            output.push_str(&format!("  Provider:     {compression_provider}\n"));
        }
    }

    let auxiliary = config_get(&config, &["auxiliary"]);
    if has_auxiliary_overrides(auxiliary) {
        output.push_str("\n◆ Auxiliary Models (overrides)\n");
        for (label, key) in [("Vision", "vision"), ("Web extract", "web_extract")] {
            let provider = scalar_or_default(&config, &["auxiliary", key, "provider"], "auto");
            let model = scalar_or_default(&config, &["auxiliary", key, "model"], "");
            if provider != "auto" || !model.is_empty() {
                let mut parts = vec![format!("provider={provider}")];
                if !model.is_empty() {
                    parts.push(format!("model={model}"));
                }
                output.push_str(&format!("  {label:12}  {}\n", parts.join(", ")));
            }
        }
    }

    output.push_str("\n◆ Messaging Platforms\n");
    output.push_str(&format!(
        "  Telegram:     {}\n",
        if has_env_value("TELEGRAM_BOT_TOKEN", &env_file) {
            "configured"
        } else {
            "not configured"
        }
    ));
    output.push_str(&format!(
        "  Discord:      {}\n",
        if has_env_value("DISCORD_BOT_TOKEN", &env_file) {
            "configured"
        } else {
            "not configured"
        }
    ));

    output.push_str("\n────────────────────────────────────────────────────────────\n");
    output.push_str("  hermes config edit     # Edit config file\n");
    output.push_str("  hermes config set <key> <value>\n");
    output.push_str("  hermes setup           # Run setup wizard\n\n");

    ConfigCommandOutcome {
        output,
        exit_code: 0,
    }
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

fn load_display_config(path: &Path) -> (Value, Option<String>) {
    let mut config = default_display_config();
    let Some(content) = fs::read_to_string(path).ok() else {
        return (config, None);
    };
    match serde_yaml::from_str::<Value>(&content) {
        Ok(Value::Mapping(mut user_config)) => {
            if let Some(max_turns) = user_config.remove(&Value::String("max_turns".to_string())) {
                let agent_key = Value::String("agent".to_string());
                let agent = user_config
                    .entry(agent_key)
                    .or_insert_with(|| Value::Mapping(Mapping::new()));
                if let Value::Mapping(agent_mapping) = agent {
                    agent_mapping
                        .entry(Value::String("max_turns".to_string()))
                        .or_insert(max_turns);
                }
            }
            deep_merge(&mut config, Value::Mapping(user_config));
            (config, None)
        }
        Ok(Value::Null) => (config, None),
        Ok(_) => (config, Some("config root is not a mapping".to_string())),
        Err(err) => (config, Some(err.to_string())),
    }
}

fn default_display_config() -> Value {
    serde_yaml::from_str(
        r#"
model: ""
agent:
  max_turns: 90
display:
  personality: kawaii
  show_reasoning: false
  bell_on_complete: false
  user_message_preview:
    first_lines: 2
    last_lines: 2
terminal:
  backend: local
  cwd: "."
  timeout: 180
  docker_image: nikolaik/python-nodejs:python3.11-nodejs20
  singularity_image: docker://nikolaik/python-nodejs:python3.11-nodejs20
  modal_image: nikolaik/python-nodejs:python3.11-nodejs20
  daytona_image: nikolaik/python-nodejs:python3.11-nodejs20
  vercel_runtime: node24
timezone: ""
compression:
  enabled: true
  threshold: 0.50
  target_ratio: 0.20
  protect_last_n: 20
auxiliary:
  compression:
    provider: auto
    model: ""
  vision:
    provider: auto
    model: ""
  web_extract:
    provider: auto
    model: ""
"#,
    )
    .expect("default display config is valid YAML")
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Mapping(base_mapping), Value::Mapping(overlay_mapping)) => {
            for (key, overlay_value) in overlay_mapping {
                match base_mapping.get_mut(&key) {
                    Some(base_value) => deep_merge(base_value, overlay_value),
                    None => {
                        base_mapping.insert(key, overlay_value);
                    }
                }
            }
        }
        (base_value, overlay_value) => *base_value = overlay_value,
    }
}

fn config_get<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current
            .as_mapping()?
            .get(&Value::String((*part).to_string()))?;
    }
    Some(current)
}

fn scalar_or_default(config: &Value, path: &[&str], default: &str) -> String {
    config_get(config, path)
        .map(display_show_value)
        .filter(|value| value != "null")
        .unwrap_or_else(|| default.to_string())
}

fn bool_or_default(config: &Value, path: &[&str], default: bool) -> bool {
    config_get(config, path)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn float_or_default(config: &Value, path: &[&str], default: f64) -> f64 {
    config_get(config, path)
        .and_then(|value| match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse::<f64>().ok(),
            _ => None,
        })
        .unwrap_or(default)
}

fn display_show_value(value: &Value) -> String {
    match value {
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Null => "None".to_string(),
        Value::Sequence(items) => {
            let parts = items.iter().map(display_show_value).collect::<Vec<_>>();
            format!("[{}]", parts.join(", "))
        }
        Value::Mapping(mapping) => {
            let parts = mapping
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}: {}",
                        display_show_mapping_key(key),
                        display_show_value(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Tagged(tagged) => display_show_value(&tagged.value),
    }
}

fn display_show_mapping_key(value: &Value) -> String {
    match value {
        Value::String(text) => format!("'{text}'"),
        other => display_show_value(other),
    }
}

fn has_auxiliary_overrides(auxiliary: Option<&Value>) -> bool {
    let Some(Value::Mapping(auxiliary)) = auxiliary else {
        return false;
    };
    ["vision", "web_extract"].iter().any(|key| {
        let Some(Value::Mapping(task)) = auxiliary.get(&Value::String((*key).to_string())) else {
            return false;
        };
        let provider = task
            .get(&Value::String("provider".to_string()))
            .map(display_show_value)
            .unwrap_or_else(|| "auto".to_string());
        let model = task
            .get(&Value::String("model".to_string()))
            .map(display_show_value)
            .unwrap_or_default();
        provider != "auto" || !model.is_empty()
    })
}

fn read_env_file(path: &Path) -> Mapping {
    let mut env = Mapping::new();
    let Some(content) = fs::read_to_string(path).ok() else {
        return env;
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        env.insert(
            Value::String(key.trim().to_string()),
            Value::String(unquote_env_value(value.trim())),
        );
    }
    env
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn env_value(key: &str, env_file: &Mapping) -> Option<String> {
    env::var(key).ok().or_else(|| {
        env_file
            .get(&Value::String(key.to_string()))
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn has_env_value(key: &str, env_file: &Mapping) -> bool {
    env_value(key, env_file).is_some_and(|value| !value.is_empty())
}

fn display_env_value(key: &str, env_file: &Mapping) -> String {
    env_value(key, env_file)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "(not set)".to_string())
}

fn redact_key(value: Option<&str>) -> String {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return "(not set)".to_string();
    };
    if value.len() < 12 {
        "***".to_string()
    } else {
        format!("{}...{}", &value[..4], &value[value.len() - 4..])
    }
}

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
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

const API_KEY_DISPLAY: &[(&str, &str)] = &[
    ("OPENROUTER_API_KEY", "OpenRouter"),
    ("VOICE_TOOLS_OPENAI_KEY", "OpenAI (STT/TTS)"),
    ("EXA_API_KEY", "Exa"),
    ("PARALLEL_API_KEY", "Parallel"),
    ("FIRECRAWL_API_KEY", "Firecrawl"),
    ("TAVILY_API_KEY", "Tavily"),
    ("BROWSERBASE_API_KEY", "Browserbase"),
    ("BROWSER_USE_API_KEY", "Browser Use"),
    ("FAL_KEY", "FAL"),
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

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_yaml::{Mapping, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginsOutcome {
    pub output: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PluginEntry {
    name: String,
    version: String,
    description: String,
    source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PluginSets {
    enabled: BTreeSet<String>,
    disabled: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PluginManifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: Value,
    #[serde(default)]
    description: String,
}

pub fn run_plugins_command(args: &[OsString], hermes_home: &Path) -> PluginsOutcome {
    match args.get(1).map(|arg| arg.to_string_lossy()) {
        Some(action) if action == "list" || action == "ls" => PluginsOutcome {
            output: render_plugins_list(hermes_home),
            exit_code: 0,
        },
        Some(action) if action == "enable" => {
            let Some(name) = args.get(2).map(|arg| arg.to_string_lossy().into_owned()) else {
                return PluginsOutcome {
                    output: "usage: hermes plugins enable <name>\n".to_string(),
                    exit_code: 2,
                };
            };
            update_plugin_enabled(hermes_home, &name, true)
        }
        Some(action) if action == "disable" => {
            let Some(name) = args.get(2).map(|arg| arg.to_string_lossy().into_owned()) else {
                return PluginsOutcome {
                    output: "usage: hermes plugins disable <name>\n".to_string(),
                    exit_code: 2,
                };
            };
            update_plugin_enabled(hermes_home, &name, false)
        }
        Some(action) => PluginsOutcome {
            output: format!(
                "HERMES_RUNTIME=rust selected, but plugins action {action:?} is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n"
            ),
            exit_code: 78,
        },
        None => PluginsOutcome {
            output: "HERMES_RUNTIME=rust selected, but interactive plugins configuration is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n".to_string(),
            exit_code: 78,
        },
    }
}

pub fn render_plugins_list(hermes_home: &Path) -> String {
    let entries = discover_all_plugins(hermes_home);
    if entries.is_empty() {
        return "No plugins installed.\nInstall with: hermes plugins install owner/repo\n"
            .to_string();
    }

    let sets = read_plugin_sets(hermes_home);
    let rows = entries
        .iter()
        .map(|entry| {
            let status = if sets.disabled.contains(&entry.name) {
                "disabled"
            } else if sets.enabled.contains(&entry.name) {
                "enabled"
            } else {
                "not enabled"
            };
            vec![
                entry.name.clone(),
                status.to_string(),
                entry.version.clone(),
                entry.description.clone(),
                entry.source.clone(),
            ]
        })
        .collect::<Vec<_>>();

    let mut output = String::new();
    output.push('\n');
    output.push_str(&render_table(
        "Plugins",
        &["Name", "Status", "Version", "Description", "Source"],
        &rows,
    ));
    output.push('\n');
    output.push('\n');
    output.push_str("Interactive toggle: hermes plugins\n");
    output.push_str("Enable/disable: hermes plugins enable/disable <name>\n");
    output.push_str("Plugins are opt-in by default \u{2014} only 'enabled' plugins load.\n");
    output
}

fn update_plugin_enabled(hermes_home: &Path, name: &str, enable: bool) -> PluginsOutcome {
    if !plugin_exists(hermes_home, name) {
        return PluginsOutcome {
            output: format!("Plugin '{name}' is not installed or bundled.\n"),
            exit_code: 1,
        };
    }

    let mut sets = read_plugin_sets(hermes_home);
    if enable {
        if sets.enabled.contains(name) && !sets.disabled.contains(name) {
            return PluginsOutcome {
                output: format!("Plugin '{name}' is already enabled.\n"),
                exit_code: 0,
            };
        }
        sets.enabled.insert(name.to_string());
        sets.disabled.remove(name);
        if let Err(message) = write_plugin_sets(hermes_home, &sets) {
            return PluginsOutcome {
                output: format!("Error: {message}\n"),
                exit_code: 1,
            };
        }
        PluginsOutcome {
            output: format!("\u{2713} Plugin {name} enabled. Takes effect on next session.\n"),
            exit_code: 0,
        }
    } else {
        if !sets.enabled.contains(name) && sets.disabled.contains(name) {
            return PluginsOutcome {
                output: format!("Plugin '{name}' is already disabled.\n"),
                exit_code: 0,
            };
        }
        sets.enabled.remove(name);
        sets.disabled.insert(name.to_string());
        if let Err(message) = write_plugin_sets(hermes_home, &sets) {
            return PluginsOutcome {
                output: format!("Error: {message}\n"),
                exit_code: 1,
            };
        }
        PluginsOutcome {
            output: format!("\u{2298} Plugin {name} disabled. Takes effect on next session.\n"),
            exit_code: 0,
        }
    }
}

fn discover_all_plugins(hermes_home: &Path) -> Vec<PluginEntry> {
    let mut seen = Vec::<PluginEntry>::new();
    for (base, source) in [
        (bundled_plugins_dir(), "bundled".to_string()),
        (hermes_home.join("plugins"), "user".to_string()),
    ] {
        let Ok(children) = fs::read_dir(base) else {
            continue;
        };
        let mut children = children.filter_map(Result::ok).collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            if !path.is_dir() {
                continue;
            }
            let dirname = child.file_name().to_string_lossy().into_owned();
            if source == "bundled" && matches!(dirname.as_str(), "memory" | "context_engine") {
                continue;
            }
            let Some(manifest_path) = manifest_path(&path) else {
                continue;
            };
            let manifest = read_manifest(&manifest_path);
            let name = if manifest.name.is_empty() {
                dirname
            } else {
                manifest.name
            };
            if source == "bundled" && seen.iter().any(|entry| entry.name == name) {
                continue;
            }
            let source_label = if source == "user" && path.join(".git").exists() {
                "git".to_string()
            } else {
                source.clone()
            };
            let entry = PluginEntry {
                name: name.clone(),
                version: value_to_string(&manifest.version),
                description: manifest.description,
                source: source_label,
            };
            if let Some(existing) = seen.iter_mut().find(|entry| entry.name == name) {
                *existing = entry;
            } else {
                seen.push(entry);
            }
        }
    }
    seen
}

fn plugin_exists(hermes_home: &Path, name: &str) -> bool {
    let user_dir = hermes_home.join("plugins");
    if user_dir.join(name).is_dir() {
        return true;
    }
    if let Ok(children) = fs::read_dir(&user_dir) {
        for child in children.filter_map(Result::ok) {
            let path = child.path();
            if !path.is_dir() {
                continue;
            }
            if let Some(manifest_path) = manifest_path(&path) {
                if read_manifest(&manifest_path).name == name {
                    return true;
                }
            }
        }
    }

    let candidate = bundled_plugins_dir().join(name);
    candidate.is_dir() && manifest_path(&candidate).is_some()
}

fn bundled_plugins_dir() -> PathBuf {
    if let Some(path) = env::var_os("HERMES_BUNDLED_PLUGINS").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join("plugins")
}

fn manifest_path(plugin_dir: &Path) -> Option<PathBuf> {
    let yaml = plugin_dir.join("plugin.yaml");
    if yaml.exists() {
        return Some(yaml);
    }
    let yml = plugin_dir.join("plugin.yml");
    if yml.exists() {
        return Some(yml);
    }
    None
}

fn read_manifest(path: &Path) -> PluginManifest {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_yaml::from_str(&content).ok())
        .unwrap_or_default()
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn read_plugin_sets(hermes_home: &Path) -> PluginSets {
    let config_path = hermes_home.join("config.yaml");
    let Ok(content) = fs::read_to_string(config_path) else {
        return PluginSets::default();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&content) else {
        return PluginSets::default();
    };
    let plugins = value
        .get("plugins")
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();
    PluginSets {
        enabled: string_set_from_mapping(&plugins, "enabled"),
        disabled: string_set_from_mapping(&plugins, "disabled"),
    }
}

fn string_set_from_mapping(mapping: &Mapping, key: &str) -> BTreeSet<String> {
    mapping
        .get(Value::String(key.to_string()))
        .and_then(Value::as_sequence)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn write_plugin_sets(hermes_home: &Path, sets: &PluginSets) -> Result<(), String> {
    fs::create_dir_all(hermes_home).map_err(|err| err.to_string())?;
    let config_path = hermes_home.join("config.yaml");
    let mut root = fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| serde_yaml::from_str::<Value>(&content).ok())
        .filter(Value::is_mapping)
        .unwrap_or_else(|| Value::Mapping(Mapping::new()));

    let root_mapping = root
        .as_mapping_mut()
        .ok_or_else(|| "config root is not a mapping".to_string())?;
    let plugins_key = Value::String("plugins".to_string());
    if !root_mapping.contains_key(&plugins_key) {
        root_mapping.insert(plugins_key.clone(), Value::Mapping(Mapping::new()));
    }
    let plugins = root_mapping
        .get_mut(&plugins_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| "plugins config is not a mapping".to_string())?;
    plugins.insert(
        Value::String("enabled".to_string()),
        Value::Sequence(
            sets.enabled
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    plugins.insert(
        Value::String("disabled".to_string()),
        Value::Sequence(
            sets.disabled
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );

    let content = serde_yaml::to_string(&root).map_err(|err| err.to_string())?;
    fs::write(config_path, content).map_err(|err| err.to_string())
}

fn render_table(title: &str, headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths = headers
        .iter()
        .map(|header| display_width(header))
        .collect::<Vec<_>>();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(display_width(cell));
        }
    }

    let total_width = widths.iter().map(|width| width + 2).sum::<usize>() + widths.len() + 1;
    let mut output = String::new();
    output.push_str(&center(title, total_width));
    output.push('\n');
    output.push('┏');
    output.push_str(&join_rule(&widths, '━', '┳'));
    output.push('┓');
    output.push('\n');
    output.push('┃');
    for (idx, header) in headers.iter().enumerate() {
        output.push_str(&format_cell(header, widths[idx]));
        output.push('┃');
    }
    output.push('\n');
    output.push('┡');
    output.push_str(&join_rule(&widths, '━', '╇'));
    output.push('┩');
    output.push('\n');
    for row in rows {
        output.push('│');
        for (idx, cell) in row.iter().enumerate() {
            output.push_str(&format_cell(cell, widths[idx]));
            output.push('│');
        }
        output.push('\n');
    }
    output.push('└');
    output.push_str(&join_rule(&widths, '─', '┴'));
    output.push('┘');
    output
}

fn join_rule(widths: &[usize], fill: char, separator: char) -> String {
    let mut output = String::new();
    for (idx, width) in widths.iter().enumerate() {
        if idx > 0 {
            output.push(separator);
        }
        output.extend(std::iter::repeat(fill).take(width + 2));
    }
    output
}

fn format_cell(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(display_width(value));
    format!(" {value}{} ", " ".repeat(padding))
}

fn center(value: &str, width: usize) -> String {
    let value_width = display_width(value);
    if value_width >= width {
        return value.to_string();
    }
    let left = (width - value_width) / 2;
    let right = width - value_width - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_rich_style_table() {
        let rows = vec![vec![
            "one".to_string(),
            "not enabled".to_string(),
            "1.0".to_string(),
            "Example".to_string(),
            "user".to_string(),
        ]];
        let rendered = render_table(
            "Plugins",
            &["Name", "Status", "Version", "Description", "Source"],
            &rows,
        );
        assert!(rendered.contains("┏━━━━━━┳━━━━━━━━━━━━━"));
        assert!(rendered.contains("│ one  │ not enabled │ 1.0"));
    }

    #[test]
    fn yaml_sets_round_trip_sorted() {
        let dir = std::env::temp_dir().join(format!("hermes-plugin-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let sets = PluginSets {
            enabled: BTreeSet::from(["z".to_string(), "a".to_string()]),
            disabled: BTreeSet::from(["m".to_string()]),
        };
        write_plugin_sets(&dir, &sets).unwrap();
        assert_eq!(read_plugin_sets(&dir), sets);
        let _ = fs::remove_dir_all(&dir);
    }
}

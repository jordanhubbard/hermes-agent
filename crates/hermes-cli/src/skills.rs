use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_yaml::{Mapping, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillsOutcome {
    pub output: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SkillEntry {
    name: String,
    category: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct HubLock {
    #[serde(default)]
    installed: BTreeMap<String, HubEntry>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct HubEntry {
    #[serde(default)]
    source: String,
    #[serde(default)]
    trust_level: String,
}

pub fn run_skills_command(args: &[OsString], hermes_home: &Path) -> SkillsOutcome {
    let Some(action) = args.get(1).map(|arg| arg.to_string_lossy()) else {
        return SkillsOutcome {
            output: "HERMES_RUNTIME=rust selected, but interactive skills configuration is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n".to_string(),
            exit_code: 78,
        };
    };
    if action != "list" {
        return SkillsOutcome {
            output: format!(
                "HERMES_RUNTIME=rust selected, but skills action {action:?} is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n"
            ),
            exit_code: 78,
        };
    }

    match parse_skills_list_args(args) {
        Ok(request) => SkillsOutcome {
            output: render_skills_list(hermes_home, &request.source_filter, request.enabled_only),
            exit_code: 0,
        },
        Err(message) => SkillsOutcome {
            output: format!("{message}\n"),
            exit_code: 2,
        },
    }
}

struct SkillsListRequest {
    source_filter: String,
    enabled_only: bool,
}

fn parse_skills_list_args(args: &[OsString]) -> Result<SkillsListRequest, String> {
    let mut source_filter = "all".to_string();
    let mut enabled_only = false;
    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--enabled-only") {
            enabled_only = true;
            i += 1;
            continue;
        }
        if arg == OsStr::new("--source") {
            let Some(value) = args
                .get(i + 1)
                .map(|arg| arg.to_string_lossy().into_owned())
            else {
                return Err("argument --source requires a value".to_string());
            };
            validate_source(&value)?;
            source_filter = value;
            i += 2;
            continue;
        }
        if let Some(value) = arg.to_string_lossy().strip_prefix("--source=") {
            validate_source(value)?;
            source_filter = value.to_string();
            i += 1;
            continue;
        }
        return Err(format!(
            "unknown skills list option: {}",
            arg.to_string_lossy()
        ));
    }
    Ok(SkillsListRequest {
        source_filter,
        enabled_only,
    })
}

fn validate_source(value: &str) -> Result<(), String> {
    match value {
        "all" | "hub" | "builtin" | "local" => Ok(()),
        other => Err(format!(
            "invalid --source value: {other}. Use all, hub, builtin, or local."
        )),
    }
}

pub fn render_skills_list(hermes_home: &Path, source_filter: &str, enabled_only: bool) -> String {
    ensure_hub_dirs(hermes_home);
    let skills_dir = hermes_home.join("skills");
    let hub_installed = read_hub_lock(&skills_dir);
    let builtin_names = read_bundled_manifest(&skills_dir);
    let disabled_names = read_disabled_skills(hermes_home);
    let all_skills = find_all_skills(hermes_home, &skills_dir);

    let mut rows = Vec::new();
    let mut hub_count = 0;
    let mut builtin_count = 0;
    let mut local_count = 0;
    let mut enabled_count = 0;
    let mut disabled_count = 0;

    for skill in all_skills {
        let (source_type, source_display, trust) =
            if let Some(entry) = hub_installed.get(&skill.name) {
                (
                    "hub",
                    if entry.source.is_empty() {
                        "hub".to_string()
                    } else {
                        entry.source.clone()
                    },
                    if entry.trust_level.is_empty() {
                        "community".to_string()
                    } else {
                        entry.trust_level.clone()
                    },
                )
            } else if builtin_names.contains(&skill.name) {
                ("builtin", "builtin".to_string(), "builtin".to_string())
            } else {
                ("local", "local".to_string(), "local".to_string())
            };

        if source_filter != "all" && source_filter != source_type {
            continue;
        }

        let is_enabled = !disabled_names.contains(&skill.name);
        if enabled_only && !is_enabled {
            continue;
        }

        match source_type {
            "hub" => hub_count += 1,
            "builtin" => builtin_count += 1,
            _ => local_count += 1,
        }
        let status = if is_enabled {
            enabled_count += 1;
            "enabled"
        } else {
            disabled_count += 1;
            "disabled"
        };
        let trust_label = if source_display == "official" {
            "official".to_string()
        } else {
            trust
        };
        rows.push(vec![
            skill.name,
            skill.category,
            source_display,
            trust_label,
            status.to_string(),
        ]);
    }

    let title = if enabled_only {
        "Installed Skills (enabled only)"
    } else {
        "Installed Skills"
    };
    let mut output = String::new();
    output.push_str(&render_table(
        title,
        &["Name", "Category", "Source", "Trust", "Status"],
        &rows,
    ));
    output.push('\n');
    if enabled_only {
        output.push_str(&format!(
            "{hub_count} hub-installed, {builtin_count} builtin, {local_count} local \u{2014} {enabled_count} enabled shown\n\n"
        ));
    } else {
        output.push_str(&format!(
            "{hub_count} hub-installed, {builtin_count} builtin, {local_count} local \u{2014} {enabled_count} enabled, {disabled_count} disabled\n\n"
        ));
    }
    output
}

fn ensure_hub_dirs(hermes_home: &Path) {
    let hub_dir = hermes_home.join("skills").join(".hub");
    let _ = fs::create_dir_all(hub_dir.join("quarantine"));
    let _ = fs::create_dir_all(hub_dir.join("index-cache"));
    let lock_file = hub_dir.join("lock.json");
    if !lock_file.exists() {
        let _ = fs::write(&lock_file, "{\"version\": 1, \"installed\": {}}\n");
    }
    let taps_file = hub_dir.join("taps.json");
    if !taps_file.exists() {
        let _ = fs::write(&taps_file, "{\"taps\": []}\n");
    }
    let audit_log = hub_dir.join("audit.log");
    if !audit_log.exists() {
        let _ = fs::write(audit_log, "");
    }
}

fn read_hub_lock(skills_dir: &Path) -> BTreeMap<String, HubEntry> {
    let lock_file = skills_dir.join(".hub").join("lock.json");
    fs::read_to_string(lock_file)
        .ok()
        .and_then(|content| serde_json::from_str::<HubLock>(&content).ok())
        .map(|lock| lock.installed)
        .unwrap_or_default()
}

fn read_bundled_manifest(skills_dir: &Path) -> BTreeSet<String> {
    let manifest = skills_dir.join(".bundled_manifest");
    let Ok(content) = fs::read_to_string(manifest) else {
        return BTreeSet::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.split_once(':').map(|(name, _)| name).unwrap_or(line))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

fn read_disabled_skills(hermes_home: &Path) -> BTreeSet<String> {
    let config_path = hermes_home.join("config.yaml");
    let Ok(content) = fs::read_to_string(config_path) else {
        return BTreeSet::new();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&content) else {
        return BTreeSet::new();
    };
    let skills = value
        .get("skills")
        .and_then(Value::as_mapping)
        .cloned()
        .unwrap_or_default();

    if let Some(platform) = env::var_os("HERMES_PLATFORM")
        .filter(|value| !value.is_empty())
        .and_then(|value| value.into_string().ok())
    {
        if let Some(platform_disabled) = skills
            .get(Value::String("platform_disabled".to_string()))
            .and_then(Value::as_mapping)
            .and_then(|mapping| mapping.get(Value::String(platform)))
        {
            return value_to_string_set(platform_disabled);
        }
    }

    skills
        .get(Value::String("disabled".to_string()))
        .map(value_to_string_set)
        .unwrap_or_default()
}

fn value_to_string_set(value: &Value) -> BTreeSet<String> {
    match value {
        Value::String(text) => BTreeSet::from([text.trim().to_string()]),
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn find_all_skills(hermes_home: &Path, skills_dir: &Path) -> Vec<SkillEntry> {
    let mut scan_dirs = Vec::new();
    if skills_dir.exists() {
        scan_dirs.push(skills_dir.to_path_buf());
    }
    scan_dirs.extend(read_external_skill_dirs(hermes_home, skills_dir));

    let mut seen = BTreeSet::new();
    let mut skills = Vec::new();
    for scan_dir in scan_dirs {
        let mut skill_files = Vec::new();
        collect_skill_files(&scan_dir, &mut skill_files);
        skill_files.sort_by_key(|path| {
            path.strip_prefix(&scan_dir)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.to_path_buf())
        });
        for skill_md in skill_files {
            let Some(skill) = read_skill_entry(&skill_md, &scan_dir) else {
                continue;
            };
            if seen.insert(skill.name.clone()) {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|left, right| {
        (left.category.as_str(), left.name.as_str())
            .cmp(&(right.category.as_str(), right.name.as_str()))
    });
    skills
}

fn read_external_skill_dirs(hermes_home: &Path, skills_dir: &Path) -> Vec<PathBuf> {
    let config_path = hermes_home.join("config.yaml");
    let Ok(content) = fs::read_to_string(config_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_yaml::from_str::<Value>(&content) else {
        return Vec::new();
    };
    let Some(raw_dirs) = value
        .get("skills")
        .and_then(Value::as_mapping)
        .and_then(|skills| skills.get(Value::String("external_dirs".to_string())))
    else {
        return Vec::new();
    };
    let values = match raw_dirs {
        Value::String(value) => vec![value.clone()],
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };
    let local_skills = skills_dir.canonicalize().ok();
    let mut seen = BTreeSet::new();
    let mut dirs = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let expanded = expand_path(trimmed, hermes_home);
        let Ok(resolved) = expanded.canonicalize() else {
            continue;
        };
        if local_skills.as_ref() == Some(&resolved) {
            continue;
        }
        if resolved.is_dir() && seen.insert(resolved.clone()) {
            dirs.push(resolved);
        }
    }
    dirs
}

fn expand_path(value: &str, hermes_home: &Path) -> PathBuf {
    let home = env::var_os("HOME").map(PathBuf::from);
    let mut expanded = if value == "~" {
        home.unwrap_or_else(|| PathBuf::from(value))
    } else if let Some(rest) = value.strip_prefix("~/") {
        home.map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(value))
    } else {
        PathBuf::from(value)
    };
    if !expanded.is_absolute() {
        expanded = hermes_home.join(expanded);
    }
    expanded
}

fn collect_skill_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name();
        if path.is_dir() {
            if matches!(
                file_name.to_str(),
                Some(".git" | ".github" | ".hub" | ".archive")
            ) {
                continue;
            }
            collect_skill_files(&path, out);
            continue;
        }
        if path.file_name() == Some(OsStr::new("SKILL.md")) {
            out.push(path);
        }
    }
}

fn read_skill_entry(skill_md: &Path, skills_dir: &Path) -> Option<SkillEntry> {
    let content = fs::read_to_string(skill_md).ok()?;
    let (frontmatter, _body) = parse_frontmatter(&content);
    if !skill_matches_platform(&frontmatter) {
        return None;
    }
    let fallback = skill_md
        .parent()?
        .file_name()?
        .to_string_lossy()
        .into_owned();
    let name = frontmatter
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&fallback)
        .chars()
        .take(64)
        .collect::<String>();
    let category = category_from_path(skill_md, skills_dir).unwrap_or_default();
    Some(SkillEntry { name, category })
}

fn parse_frontmatter(content: &str) -> (Mapping, String) {
    if !content.starts_with("---") {
        return (Mapping::new(), content.to_string());
    }
    let rest = &content[3..];
    let Some(end) = rest.find("\n---") else {
        return (Mapping::new(), content.to_string());
    };
    let yaml_content = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest
        .get(body_start..)
        .unwrap_or_default()
        .trim_start_matches('\n')
        .to_string();
    let frontmatter = serde_yaml::from_str::<Value>(yaml_content)
        .ok()
        .and_then(|value| value.as_mapping().cloned())
        .unwrap_or_default();
    (frontmatter, body)
}

fn skill_matches_platform(frontmatter: &Mapping) -> bool {
    let Some(platforms) = frontmatter.get(Value::String("platforms".to_string())) else {
        return true;
    };
    let values = match platforms {
        Value::String(value) => vec![value.clone()],
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => return true,
    };
    if values.is_empty() {
        return true;
    }
    let current = env::consts::OS;
    values.into_iter().any(|platform| {
        let normalized = platform.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "macos" | "darwin" => current == "macos",
            "windows" | "win32" => current == "windows",
            "linux" => current == "linux",
            other => current.starts_with(other),
        }
    })
}

fn category_from_path(skill_md: &Path, skills_dir: &Path) -> Option<String> {
    let rel = skill_md.strip_prefix(skills_dir).ok()?;
    let parts = rel.components().collect::<Vec<_>>();
    if parts.len() >= 3 {
        return parts[0].as_os_str().to_str().map(str::to_string);
    }
    None
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

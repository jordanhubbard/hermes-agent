use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};

use hermes_config::{path_semantics, PathSemantics};
use serde::Serialize;
use serde_yaml::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RustProfileContext {
    pub args: Vec<OsString>,
    pub active_profile: String,
    pub hermes_home: PathBuf,
    pub paths: PathSemantics,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProfileStatus {
    pub active_profile: String,
    pub path: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub gateway_running: bool,
    pub skill_count: Option<usize>,
    pub alias_path: Option<String>,
}

pub fn resolve_rust_profile_context(args: &[OsString]) -> Result<RustProfileContext, String> {
    let home = home_dir();
    let env_home = env::var_os("HERMES_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let base_paths = path_semantics(&home, env_home.as_deref());
    let default_root = PathBuf::from(&base_paths.default_hermes_root);
    let profiles_root = PathBuf::from(&base_paths.profiles_root);
    let active_profile_path = PathBuf::from(&base_paths.active_profile_path);
    let (stripped_args, explicit_profile) = strip_profile_args(args)?;

    let (active_profile, hermes_home) = if let Some(profile) = explicit_profile {
        let canon = normalize_profile_name(&profile)?;
        let profile_home = profile_dir(&default_root, &profiles_root, &canon);
        if canon != "default" && !profile_home.is_dir() {
            return Err(format!(
                "Profile '{canon}' does not exist. Create it with: hermes profile create {canon}"
            ));
        }
        (canon, profile_home)
    } else if let Some(hermes_home) = env_home {
        (
            infer_active_profile(&hermes_home, &default_root, &profiles_root),
            hermes_home,
        )
    } else if let Some(active) = read_sticky_active_profile(&active_profile_path)? {
        let profile_home = profile_dir(&default_root, &profiles_root, &active);
        if active != "default" && !profile_home.is_dir() {
            return Err(format!(
                "Profile '{active}' does not exist. Create it with: hermes profile create {active}"
            ));
        }
        (active, profile_home)
    } else {
        ("default".to_string(), default_root)
    };

    let paths = path_semantics(&home, Some(&hermes_home));
    Ok(RustProfileContext {
        args: stripped_args,
        active_profile,
        hermes_home,
        paths,
    })
}

pub fn profile_status(context: &RustProfileContext) -> ProfileStatus {
    let active_profile = infer_active_profile(
        &context.hermes_home,
        Path::new(&context.paths.default_hermes_root),
        Path::new(&context.paths.profiles_root),
    );
    let (model, provider) = read_config_model(&context.hermes_home);
    ProfileStatus {
        active_profile,
        path: context.paths.display_hermes_home.clone(),
        model,
        provider,
        gateway_running: false,
        skill_count: context
            .hermes_home
            .is_dir()
            .then(|| count_skills(&context.hermes_home)),
        alias_path: alias_path_for(&context.active_profile),
    }
}

pub fn render_profile_status(status: &ProfileStatus) -> String {
    let mut output = String::new();
    output.push('\n');
    output.push_str(&format!("Active profile: {}\n", status.active_profile));
    output.push_str(&format!("Path:           {}\n", status.path));
    if let Some(model) = &status.model {
        output.push_str("Model:          ");
        output.push_str(model);
        if let Some(provider) = &status.provider {
            output.push_str(" (");
            output.push_str(provider);
            output.push(')');
        }
        output.push('\n');
    }
    if let Some(skill_count) = status.skill_count {
        output.push_str(&format!(
            "Gateway:        {}\n",
            if status.gateway_running {
                "running"
            } else {
                "stopped"
            }
        ));
        output.push_str(&format!("Skills:         {skill_count} installed\n"));
        if let Some(_alias_path) = &status.alias_path {
            output.push_str(&format!(
                "Alias:          {} → hermes -p {}\n",
                status.active_profile, status.active_profile
            ));
        }
    }
    output.push('\n');
    output
}

fn strip_profile_args(args: &[OsString]) -> Result<(Vec<OsString>, Option<String>), String> {
    let mut stripped = Vec::with_capacity(args.len());
    let mut profile = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("-p") || arg == OsStr::new("--profile") {
            let Some(value) = args.get(i + 1) else {
                return Err(format!("{} requires a profile name", arg.to_string_lossy()));
            };
            if profile.is_none() {
                profile = Some(value.to_string_lossy().into_owned());
            }
            i += 2;
            continue;
        }
        if let Some(value) = arg
            .to_string_lossy()
            .strip_prefix("--profile=")
            .map(str::to_string)
        {
            if profile.is_none() {
                profile = Some(value);
            }
            i += 1;
            continue;
        }
        stripped.push(arg.clone());
        i += 1;
    }
    Ok((stripped, profile))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn normalize_profile_name(name: &str) -> Result<String, String> {
    let stripped = name.trim();
    if stripped.is_empty() {
        return Err("profile name cannot be empty".to_string());
    }
    let canon = if stripped.eq_ignore_ascii_case("default") {
        "default".to_string()
    } else {
        stripped.to_ascii_lowercase()
    };
    validate_profile_name(&canon)?;
    Ok(canon)
}

fn validate_profile_name(name: &str) -> Result<(), String> {
    if name == "default" {
        return Ok(());
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return Err("profile name cannot be empty".to_string());
    };
    let valid_first = first.is_ascii_lowercase() || first.is_ascii_digit();
    let valid_rest =
        chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-');
    if name.len() <= 64 && valid_first && valid_rest {
        Ok(())
    } else {
        Err(format!(
            "Invalid profile name {name:?}. Must match [a-z0-9][a-z0-9_-]{{0,63}}"
        ))
    }
}

fn profile_dir(default_root: &Path, profiles_root: &Path, name: &str) -> PathBuf {
    if name == "default" {
        default_root.to_path_buf()
    } else {
        profiles_root.join(name)
    }
}

fn read_sticky_active_profile(path: &Path) -> Result<Option<String>, String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let name = raw.trim();
    if name.is_empty() || name == "default" {
        return Ok(None);
    }
    normalize_profile_name(name).map(Some)
}

fn infer_active_profile(hermes_home: &Path, default_root: &Path, profiles_root: &Path) -> String {
    if same_path(hermes_home, default_root) {
        return "default".to_string();
    }
    if let Ok(relative) = hermes_home.strip_prefix(profiles_root) {
        let parts: Vec<_> = relative.components().collect();
        if parts.len() == 1 {
            if let Component::Normal(value) = parts[0] {
                let text = value.to_string_lossy();
                if validate_profile_name(&text).is_ok() && text != "default" {
                    return text.into_owned();
                }
            }
        }
    }
    "custom".to_string()
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_components(left) == normalize_components(right)
}

fn normalize_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(value) => Some(value.as_os_str().to_string_lossy().to_string()),
            Component::RootDir => Some("/".to_string()),
            Component::CurDir => None,
            Component::ParentDir => Some("..".to_string()),
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
        })
        .collect()
}

fn read_config_model(profile_dir: &Path) -> (Option<String>, Option<String>) {
    let config_path = profile_dir.join("config.yaml");
    let Ok(raw) = fs::read_to_string(config_path) else {
        return (None, None);
    };
    let Ok(root) = serde_yaml::from_str::<Value>(&raw) else {
        return (None, None);
    };
    let Some(model) = mapping_get(&root, "model") else {
        return (None, None);
    };
    if let Some(text) = scalar_to_string(model) {
        return (Some(text), None);
    }
    let default = mapping_get(model, "default")
        .or_else(|| mapping_get(model, "model"))
        .and_then(scalar_to_string);
    let provider = mapping_get(model, "provider").and_then(scalar_to_string);
    (default, provider)
}

fn mapping_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Mapping(map) = value else {
        return None;
    };
    map.get(&Value::String(key.to_string()))
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn count_skills(profile_dir: &Path) -> usize {
    count_skills_under(&profile_dir.join("skills"))
}

fn count_skills_under(path: &Path) -> usize {
    let Ok(meta) = fs::metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return usize::from(
            path.file_name() == Some(OsStr::new("SKILL.md")) && !is_ignored_skill_path(path),
        );
    }
    if !meta.is_dir() || is_ignored_skill_path(path) {
        return 0;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| count_skills_under(&entry.path()))
        .sum()
}

fn is_ignored_skill_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(value) => value == OsStr::new(".hub") || value == OsStr::new(".git"),
        _ => false,
    })
}

fn alias_path_for(profile: &str) -> Option<String> {
    if profile == "default" || profile == "custom" {
        return None;
    }
    let path = home_dir().join(".local").join("bin").join(profile);
    path.exists().then(|| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_profile_names_like_python_profiles() {
        assert_eq!(normalize_profile_name("Default").unwrap(), "default");
        assert_eq!(normalize_profile_name("coder_1").unwrap(), "coder_1");
        assert!(normalize_profile_name("-bad").is_err());
        assert!(normalize_profile_name("bad!").is_err());
    }

    #[test]
    fn strips_profile_args_without_disturbing_command_args() {
        let args = vec![
            OsString::from("-p"),
            OsString::from("coder"),
            OsString::from("profile"),
        ];
        let (stripped, profile) = strip_profile_args(&args).unwrap();
        assert_eq!(stripped, vec![OsString::from("profile")]);
        assert_eq!(profile.as_deref(), Some("coder"));
    }
}

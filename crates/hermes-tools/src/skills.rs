use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const EXCLUDED_SKILL_DIRS: &[&str] = &[".git", ".github", ".hub", ".archive"];

#[derive(Clone, Debug, Eq, PartialEq)]
struct SkillRecord {
    name: String,
    description: String,
    category: Option<String>,
    skill_dir: PathBuf,
    skill_md: PathBuf,
    frontmatter: BTreeMap<String, Value>,
    content: String,
}

pub fn skills_list(skills_dir: &Path, category: Option<&str>) -> io::Result<Value> {
    if !skills_dir.exists() {
        fs::create_dir_all(skills_dir)?;
        return Ok(json!({
            "success": true,
            "skills": [],
            "categories": [],
            "message": format!("No skills found. Skills directory created at {}/", skills_dir.display()),
        }));
    }

    let mut skills = find_all_skills(skills_dir)?;
    if let Some(category) = category {
        skills.retain(|skill| skill.category.as_deref() == Some(category));
    }
    skills.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
    });
    let categories = skills
        .iter()
        .filter_map(|skill| skill.category.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let list = skills
        .iter()
        .map(|skill| {
            json!({
                "name": skill.name,
                "description": skill.description,
                "category": skill.category,
            })
        })
        .collect::<Vec<_>>();

    if list.is_empty() {
        Ok(json!({
            "success": true,
            "skills": [],
            "categories": [],
            "message": "No skills found in skills/ directory.",
        }))
    } else {
        Ok(json!({
            "success": true,
            "skills": list,
            "categories": categories,
            "count": list.len(),
            "hint": "Use skill_view(name) to see full content, tags, and linked files",
        }))
    }
}

pub fn skill_view(skills_dir: &Path, name: &str, file_path: Option<&str>) -> io::Result<Value> {
    let Some(record) = resolve_skill(skills_dir, name)? else {
        let available = find_all_skills(skills_dir)?
            .into_iter()
            .take(20)
            .map(|skill| skill.name)
            .collect::<Vec<_>>();
        return Ok(json!({
            "success": false,
            "error": format!("Skill '{name}' not found."),
            "available_skills": available,
            "hint": "Use skills_list to see all available skills",
        }));
    };

    if !skill_matches_platform(&record.frontmatter) {
        return Ok(json!({
            "success": false,
            "error": format!("Skill '{name}' is not supported on this platform."),
            "readiness_status": "unsupported",
        }));
    }

    if let Some(file_path) = file_path {
        return view_linked_file(&record, name, file_path);
    }

    let tags = parse_tags(
        record
            .frontmatter
            .get("tags")
            .cloned()
            .unwrap_or(Value::Null),
    );
    let related_skills = parse_tags(
        record
            .frontmatter
            .get("related_skills")
            .cloned()
            .unwrap_or(Value::Null),
    );
    let linked_files = linked_files(&record.skill_dir);
    let linked_files_value = if linked_files.as_object().is_some_and(|obj| obj.is_empty()) {
        Value::Null
    } else {
        linked_files
    };

    let rel_path = record
        .skill_md
        .strip_prefix(skills_dir)
        .unwrap_or(&record.skill_md)
        .to_string_lossy()
        .to_string();
    let usage_hint = if linked_files_value.is_null() {
        Value::Null
    } else {
        json!("To view linked files, call skill_view(name, file_path) where file_path is e.g. 'references/api.md' or 'assets/config.yaml'")
    };

    Ok(json!({
        "success": true,
        "name": record.name,
        "description": record.frontmatter.get("description").and_then(Value::as_str).unwrap_or(""),
        "tags": tags,
        "related_skills": related_skills,
        "content": record.content,
        "path": rel_path,
        "skill_dir": record.skill_dir.to_string_lossy(),
        "linked_files": linked_files_value,
        "usage_hint": usage_hint,
        "required_environment_variables": [],
        "required_commands": [],
        "missing_required_environment_variables": [],
        "missing_credential_files": [],
        "missing_required_commands": [],
        "setup_needed": false,
        "setup_skipped": false,
        "readiness_status": "available",
    }))
}

fn view_linked_file(record: &SkillRecord, name: &str, file_path: &str) -> io::Result<Value> {
    if has_traversal_component(file_path) {
        return Ok(json!({
            "success": false,
            "error": "Path traversal ('..') is not allowed.",
            "hint": "Use a relative path within the skill directory",
        }));
    }

    let target = record.skill_dir.join(file_path);
    let target_parent = target
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| record.skill_dir.clone());
    let skill_dir = record
        .skill_dir
        .canonicalize()
        .unwrap_or_else(|_| record.skill_dir.clone());
    if !target_parent.starts_with(&skill_dir) {
        return Ok(json!({
            "success": false,
            "error": "Path escapes skill directory.",
            "hint": "Use a relative path within the skill directory",
        }));
    }

    if !target.exists() {
        return Ok(json!({
            "success": false,
            "error": format!("File '{file_path}' not found in skill '{name}'."),
            "available_files": available_files(&record.skill_dir),
            "hint": "Use one of the available file paths listed above",
        }));
    }

    match fs::read_to_string(&target) {
        Ok(content) => Ok(json!({
            "success": true,
            "name": name,
            "file": file_path,
            "content": content,
            "file_type": target.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default(),
        })),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            let size = fs::metadata(&target).map(|meta| meta.len()).unwrap_or(0);
            Ok(json!({
                "success": true,
                "name": name,
                "file": file_path,
                "content": format!("[Binary file: {}, size: {} bytes]", target.file_name().unwrap_or_default().to_string_lossy(), size),
                "is_binary": true,
            }))
        }
        Err(error) => Err(error),
    }
}

fn find_all_skills(skills_dir: &Path) -> io::Result<Vec<SkillRecord>> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for skill_md in find_skill_files(skills_dir)? {
        let content = match fs::read_to_string(&skill_md) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let (frontmatter, body) = parse_frontmatter(&content);
        if !skill_matches_platform(&frontmatter) {
            continue;
        }
        let skill_dir = skill_md.parent().unwrap_or(skills_dir).to_path_buf();
        let mut name = frontmatter
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or(""))
            .to_string();
        if name.chars().count() > MAX_NAME_LENGTH {
            name = name.chars().take(MAX_NAME_LENGTH).collect();
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        let mut description = frontmatter
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if description.is_empty() {
            description = body
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .unwrap_or("")
                .to_string();
        }
        if description.chars().count() > MAX_DESCRIPTION_LENGTH {
            description = format!(
                "{}...",
                description
                    .chars()
                    .take(MAX_DESCRIPTION_LENGTH.saturating_sub(3))
                    .collect::<String>()
            );
        }
        records.push(SkillRecord {
            name,
            description,
            category: category_from_path(skills_dir, &skill_md),
            skill_dir,
            skill_md,
            frontmatter,
            content,
        });
    }
    records.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(records)
}

fn resolve_skill(skills_dir: &Path, name: &str) -> io::Result<Option<SkillRecord>> {
    let direct = skills_dir.join(name);
    if direct.is_dir() && direct.join("SKILL.md").exists() {
        return Ok(record_from_path(skills_dir, direct.join("SKILL.md")));
    }
    let direct_md = direct.with_extension("md");
    if direct_md.exists() {
        return Ok(record_from_path(skills_dir, direct_md));
    }
    for record in find_all_skills(skills_dir)? {
        if record
            .skill_dir
            .file_name()
            .and_then(|file_name| file_name.to_str())
            == Some(name)
            || record.name == name
        {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

fn record_from_path(skills_dir: &Path, skill_md: PathBuf) -> Option<SkillRecord> {
    let content = fs::read_to_string(&skill_md).ok()?;
    let (frontmatter, body) = parse_frontmatter(&content);
    let skill_dir = skill_md.parent().unwrap_or(skills_dir).to_path_buf();
    let name = frontmatter
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_else(|| skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or(""))
        .to_string();
    let description = frontmatter
        .get("description")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            body.lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('#'))
                .map(ToString::to_string)
        })
        .unwrap_or_default();
    Some(SkillRecord {
        name,
        description,
        category: category_from_path(skills_dir, &skill_md),
        skill_dir,
        skill_md,
        frontmatter,
        content,
    })
}

fn find_skill_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if EXCLUDED_SKILL_DIRS.contains(&name) {
            continue;
        }
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                out.push(skill_md);
            } else {
                out.extend(find_skill_files(&path)?);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn parse_frontmatter(content: &str) -> (BTreeMap<String, Value>, String) {
    if !content.starts_with("---") {
        return (BTreeMap::new(), content.to_string());
    }
    let rest = &content[3..];
    let Some(end) = rest.find("\n---") else {
        return (BTreeMap::new(), content.to_string());
    };
    let yaml = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest
        .get(body_start..)
        .unwrap_or("")
        .trim_start_matches(['\r', '\n'])
        .to_string();
    let mut out = BTreeMap::new();
    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || line.starts_with(' ') {
            continue;
        }
        out.insert(key.to_string(), parse_scalar(value.trim()));
    }
    (out, body)
}

fn parse_scalar(value: &str) -> Value {
    let value = value.trim();
    if value.starts_with('[') && value.ends_with(']') {
        let inner = &value[1..value.len().saturating_sub(1)];
        return Value::Array(
            inner
                .split(',')
                .map(|item| Value::String(strip_quotes(item.trim()).to_string()))
                .filter(|item| item.as_str().is_some_and(|text| !text.is_empty()))
                .collect(),
        );
    }
    Value::String(strip_quotes(value).to_string())
}

fn strip_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

fn skill_matches_platform(frontmatter: &BTreeMap<String, Value>) -> bool {
    let Some(platforms) = frontmatter.get("platforms") else {
        return true;
    };
    let platforms = match platforms {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        Value::String(text) => vec![text.clone()],
        _ => return true,
    };
    if platforms.is_empty() {
        return true;
    }
    let current = current_platform();
    platforms.iter().any(|platform| {
        let normalized = platform.trim().to_ascii_lowercase();
        let mapped = match normalized.as_str() {
            "macos" => "darwin",
            "linux" => "linux",
            "windows" => "win32",
            other => other,
        };
        current.starts_with(mapped)
    })
}

fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else {
        "linux"
    }
}

fn category_from_path(skills_dir: &Path, skill_path: &Path) -> Option<String> {
    let rel = skill_path.strip_prefix(skills_dir).ok()?;
    let parts = rel.components().collect::<Vec<_>>();
    if parts.len() >= 3 {
        parts[0].as_os_str().to_str().map(ToString::to_string)
    } else {
        None
    }
}

fn parse_tags(value: Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| item.as_str().map(str::trim).map(ToString::to_string))
            .filter(|item| !item.is_empty())
            .collect(),
        Value::String(text) => text
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|item| strip_quotes(item.trim()).to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn linked_files(skill_dir: &Path) -> Value {
    let mut out = serde_json::Map::new();
    let references = glob_under(skill_dir, "references", &["md"], false);
    if !references.is_empty() {
        out.insert("references".to_string(), json!(references));
    }
    let templates = glob_under(
        skill_dir,
        "templates",
        &["md", "py", "yaml", "yml", "json", "tex", "sh"],
        true,
    );
    if !templates.is_empty() {
        out.insert("templates".to_string(), json!(templates));
    }
    let assets = all_files_under(skill_dir, "assets");
    if !assets.is_empty() {
        out.insert("assets".to_string(), json!(assets));
    }
    let scripts = glob_under(
        skill_dir,
        "scripts",
        &["py", "sh", "bash", "js", "ts", "rb"],
        false,
    );
    if !scripts.is_empty() {
        out.insert("scripts".to_string(), json!(scripts));
    }
    Value::Object(out)
}

fn available_files(skill_dir: &Path) -> Value {
    let mut files = serde_json::Map::new();
    for rel in all_non_skill_files(skill_dir) {
        let key = if rel.starts_with("references/") {
            "references"
        } else if rel.starts_with("templates/") {
            "templates"
        } else if rel.starts_with("assets/") {
            "assets"
        } else if rel.starts_with("scripts/") {
            "scripts"
        } else {
            "other"
        };
        files
            .entry(key.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("entry is array")
            .push(Value::String(rel));
    }
    Value::Object(files)
}

fn glob_under(skill_dir: &Path, subdir: &str, extensions: &[&str], recursive: bool) -> Vec<String> {
    let root = skill_dir.join(subdir);
    let mut files = if recursive {
        all_files(&root)
    } else {
        fs::read_dir(&root)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>()
    };
    files.retain(|path| {
        path.extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
    });
    rel_paths(skill_dir, files)
}

fn all_files_under(skill_dir: &Path, subdir: &str) -> Vec<String> {
    rel_paths(skill_dir, all_files(&skill_dir.join(subdir)))
}

fn all_non_skill_files(skill_dir: &Path) -> Vec<String> {
    rel_paths(
        skill_dir,
        all_files(skill_dir)
            .into_iter()
            .filter(|path| path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md"))
            .collect(),
    )
}

fn all_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    if root.is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(all_files(&path));
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out
}

fn rel_paths(skill_dir: &Path, files: Vec<PathBuf>) -> Vec<String> {
    let mut rels = files
        .into_iter()
        .filter_map(|path| {
            path.strip_prefix(skill_dir)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    rels.sort();
    rels
}

fn has_traversal_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_frontmatter_and_arrays() {
        let (frontmatter, body) = parse_frontmatter(
            "---\nname: demo\ntags: [one, two]\ndescription: \"Demo\"\n---\n# Body",
        );
        assert_eq!(frontmatter["name"], "demo");
        assert_eq!(parse_tags(frontmatter["tags"].clone()), vec!["one", "two"]);
        assert_eq!(body, "# Body");
    }
}

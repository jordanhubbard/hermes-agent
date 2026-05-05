use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const MAX_SKILL_CONTENT_CHARS: usize = 100_000;
pub const MAX_SKILL_FILE_BYTES: usize = 1_048_576;
const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const ALLOWED_SUBDIRS: &[&str] = &["assets", "references", "scripts", "templates"];

#[derive(Clone, Debug, Default)]
pub struct SkillManageRequest<'a> {
    pub action: &'a str,
    pub name: &'a str,
    pub content: Option<&'a str>,
    pub category: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub file_content: Option<&'a str>,
    pub old_string: Option<&'a str>,
    pub new_string: Option<&'a str>,
    pub replace_all: bool,
    pub absorbed_into: Option<&'a str>,
}

pub fn skill_manage(skills_dir: &Path, request: SkillManageRequest<'_>) -> io::Result<Value> {
    let result = match request.action {
        "create" => {
            let Some(content) = request.content.filter(|content| !content.is_empty()) else {
                return Ok(tool_error(
                    "content is required for 'create'. Provide the full SKILL.md text (frontmatter + body).",
                ));
            };
            create_skill(skills_dir, request.name, content, request.category)?
        }
        "edit" => {
            let Some(content) = request.content.filter(|content| !content.is_empty()) else {
                return Ok(tool_error(
                    "content is required for 'edit'. Provide the full updated SKILL.md text.",
                ));
            };
            edit_skill(skills_dir, request.name, content)?
        }
        "patch" => {
            let Some(old_string) = request.old_string.filter(|value| !value.is_empty()) else {
                return Ok(tool_error(
                    "old_string is required for 'patch'. Provide the text to find.",
                ));
            };
            let Some(new_string) = request.new_string else {
                return Ok(tool_error(
                    "new_string is required for 'patch'. Use empty string to delete matched text.",
                ));
            };
            patch_skill(
                skills_dir,
                request.name,
                old_string,
                new_string,
                request.file_path,
                request.replace_all,
            )?
        }
        "delete" => delete_skill(skills_dir, request.name, request.absorbed_into)?,
        "write_file" => {
            let Some(file_path) = request.file_path.filter(|value| !value.is_empty()) else {
                return Ok(tool_error(
                    "file_path is required for 'write_file'. Example: 'references/api-guide.md'",
                ));
            };
            let Some(file_content) = request.file_content else {
                return Ok(tool_error("file_content is required for 'write_file'."));
            };
            write_file(skills_dir, request.name, file_path, file_content)?
        }
        "remove_file" => {
            let Some(file_path) = request.file_path.filter(|value| !value.is_empty()) else {
                return Ok(tool_error("file_path is required for 'remove_file'."));
            };
            remove_file(skills_dir, request.name, file_path)?
        }
        action => {
            json!({
                "success": false,
                "error": format!("Unknown action '{action}'. Use: create, edit, patch, delete, write_file, remove_file"),
            })
        }
    };
    Ok(result)
}

fn create_skill(
    skills_dir: &Path,
    name: &str,
    content: &str,
    category: Option<&str>,
) -> io::Result<Value> {
    if let Some(error) = validate_name(name) {
        return Ok(json!({"success": false, "error": error}));
    }
    if let Some(error) = validate_category(category) {
        return Ok(json!({"success": false, "error": error}));
    }
    if let Some(error) = validate_frontmatter(content) {
        return Ok(json!({"success": false, "error": error}));
    }
    if let Some(error) = validate_content_size(content, "SKILL.md") {
        return Ok(json!({"success": false, "error": error}));
    }
    if let Some(existing) = find_skill(skills_dir, name)? {
        return Ok(json!({
            "success": false,
            "error": format!("A skill named '{name}' already exists at {}.", existing.display()),
        }));
    }

    let skill_dir = resolve_skill_dir(skills_dir, name, category);
    fs::create_dir_all(&skill_dir)?;
    let skill_md = skill_dir.join("SKILL.md");
    fs::write(&skill_md, content)?;

    let mut result = json!({
        "success": true,
        "message": format!("Skill '{name}' created."),
        "path": skill_dir.strip_prefix(skills_dir).unwrap_or(&skill_dir).to_string_lossy(),
        "skill_md": skill_md.to_string_lossy(),
        "hint": format!(
            "To add reference files, templates, or scripts, use skill_manage(action='write_file', name='{name}', file_path='references/example.md', file_content='...')"
        ),
    });
    if let Some(category) = category.filter(|value| !value.trim().is_empty()) {
        result["category"] = json!(category.trim());
    }
    Ok(result)
}

fn edit_skill(skills_dir: &Path, name: &str, content: &str) -> io::Result<Value> {
    if let Some(error) = validate_frontmatter(content) {
        return Ok(json!({"success": false, "error": error}));
    }
    if let Some(error) = validate_content_size(content, "SKILL.md") {
        return Ok(json!({"success": false, "error": error}));
    }
    let Some(skill_dir) = find_skill(skills_dir, name)? else {
        return Ok(json!({
            "success": false,
            "error": format!("Skill '{name}' not found. Use skills_list() to see available skills."),
        }));
    };
    fs::write(skill_dir.join("SKILL.md"), content)?;
    Ok(json!({
        "success": true,
        "message": format!("Skill '{name}' updated."),
        "path": skill_dir.to_string_lossy(),
    }))
}

fn patch_skill(
    skills_dir: &Path,
    name: &str,
    old_string: &str,
    new_string: &str,
    file_path: Option<&str>,
    replace_all: bool,
) -> io::Result<Value> {
    let Some(skill_dir) = find_skill(skills_dir, name)? else {
        return Ok(json!({"success": false, "error": format!("Skill '{name}' not found.")}));
    };
    let target = if let Some(file_path) = file_path {
        if let Some(error) = validate_file_path(file_path) {
            return Ok(json!({"success": false, "error": error}));
        }
        let Some(target) = resolve_skill_target(&skill_dir, file_path) else {
            return Ok(json!({"success": false, "error": "Path escapes skill directory."}));
        };
        target
    } else {
        skill_dir.join("SKILL.md")
    };
    if !target.exists() {
        let rel = target
            .strip_prefix(&skill_dir)
            .unwrap_or(&target)
            .to_string_lossy();
        return Ok(json!({"success": false, "error": format!("File not found: {rel}")}));
    }
    let content = fs::read_to_string(&target)?;
    let occurrences = content.matches(old_string).count();
    if occurrences == 0 {
        let preview = if content.len() > 500 {
            format!("{}...", &content[..500])
        } else {
            content.clone()
        };
        return Ok(json!({
            "success": false,
            "error": format!("Could not find match for old_string in {}", target.file_name().and_then(|name| name.to_str()).unwrap_or("file")),
            "file_preview": preview,
        }));
    }
    if occurrences > 1 && !replace_all {
        return Ok(json!({
            "success": false,
            "error": format!("Found {occurrences} matches for old_string; set replace_all=true to replace all."),
            "file_preview": if content.len() > 500 { format!("{}...", &content[..500]) } else { content.clone() },
        }));
    }
    let new_content = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    let label = file_path.unwrap_or("SKILL.md");
    if let Some(error) = validate_content_size(&new_content, label) {
        return Ok(json!({"success": false, "error": error}));
    }
    if file_path.is_none() {
        if let Some(error) = validate_frontmatter(&new_content) {
            return Ok(json!({
                "success": false,
                "error": format!("Patch would break SKILL.md structure: {error}"),
            }));
        }
    }
    fs::write(&target, new_content)?;
    Ok(json!({
        "success": true,
        "message": format!(
            "Patched {} in skill '{name}' ({} replacement{}).",
            file_path.unwrap_or("SKILL.md"),
            occurrences,
            if occurrences > 1 { "s" } else { "" }
        ),
    }))
}

fn delete_skill(skills_dir: &Path, name: &str, absorbed_into: Option<&str>) -> io::Result<Value> {
    let Some(skill_dir) = find_skill(skills_dir, name)? else {
        return Ok(json!({"success": false, "error": format!("Skill '{name}' not found.")}));
    };
    if let Some(target) = absorbed_into
        .map(str::trim)
        .filter(|target| !target.is_empty())
    {
        if target == name {
            return Ok(json!({
                "success": false,
                "error": format!("absorbed_into='{target}' cannot equal the skill being deleted."),
            }));
        }
        if find_skill(skills_dir, target)?.is_none() {
            return Ok(json!({
                "success": false,
                "error": format!(
                    "absorbed_into='{target}' does not exist. Create or patch the umbrella skill first, then retry the delete."
                ),
            }));
        }
    }

    fs::remove_dir_all(&skill_dir)?;
    if let Some(parent) = skill_dir.parent() {
        if parent != skills_dir && parent.exists() && fs::read_dir(parent)?.next().is_none() {
            fs::remove_dir(parent)?;
        }
    }
    let mut message = format!("Skill '{name}' deleted.");
    if let Some(target) = absorbed_into
        .map(str::trim)
        .filter(|target| !target.is_empty())
    {
        message.push_str(&format!(" Content absorbed into '{target}'."));
    }
    Ok(json!({"success": true, "message": message}))
}

fn write_file(
    skills_dir: &Path,
    name: &str,
    file_path: &str,
    file_content: &str,
) -> io::Result<Value> {
    if let Some(error) = validate_file_path(file_path) {
        return Ok(json!({"success": false, "error": error}));
    }
    let byte_count = file_content.len();
    if byte_count > MAX_SKILL_FILE_BYTES {
        return Ok(json!({
            "success": false,
            "error": format!(
                "File content is {} bytes (limit: {} bytes / 1 MiB). Consider splitting into smaller files.",
                format_count(byte_count),
                format_count(MAX_SKILL_FILE_BYTES)
            ),
        }));
    }
    if let Some(error) = validate_content_size(file_content, file_path) {
        return Ok(json!({"success": false, "error": error}));
    }
    let Some(skill_dir) = find_skill(skills_dir, name)? else {
        return Ok(json!({
            "success": false,
            "error": format!("Skill '{name}' not found. Create it first with action='create'."),
        }));
    };
    let Some(target) = resolve_skill_target(&skill_dir, file_path) else {
        return Ok(json!({"success": false, "error": "Path escapes skill directory."}));
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target, file_content)?;
    Ok(json!({
        "success": true,
        "message": format!("File '{file_path}' written to skill '{name}'."),
        "path": target.to_string_lossy(),
    }))
}

fn remove_file(skills_dir: &Path, name: &str, file_path: &str) -> io::Result<Value> {
    if let Some(error) = validate_file_path(file_path) {
        return Ok(json!({"success": false, "error": error}));
    }
    let Some(skill_dir) = find_skill(skills_dir, name)? else {
        return Ok(json!({"success": false, "error": format!("Skill '{name}' not found.")}));
    };
    let Some(target) = resolve_skill_target(&skill_dir, file_path) else {
        return Ok(json!({"success": false, "error": "Path escapes skill directory."}));
    };
    if !target.exists() {
        let available = available_supporting_files(&skill_dir)?;
        return Ok(json!({
            "success": false,
            "error": format!("File '{file_path}' not found in skill '{name}'."),
            "available_files": if available.is_empty() { Value::Null } else { json!(available) },
        }));
    }
    fs::remove_file(&target)?;
    if let Some(parent) = target.parent() {
        if parent != skill_dir && parent.exists() && fs::read_dir(parent)?.next().is_none() {
            fs::remove_dir(parent)?;
        }
    }
    Ok(json!({
        "success": true,
        "message": format!("File '{file_path}' removed from skill '{name}'."),
    }))
}

fn validate_name(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("Skill name is required.".to_string());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Some(format!("Skill name exceeds {MAX_NAME_LENGTH} characters."));
    }
    if !Regex::new(r"^[a-z0-9][a-z0-9._-]*$")
        .expect("skill name regex compiles")
        .is_match(name)
    {
        return Some(format!(
            "Invalid skill name '{name}'. Use lowercase letters, numbers, hyphens, dots, and underscores. Must start with a letter or digit."
        ));
    }
    None
}

fn validate_category(category: Option<&str>) -> Option<String> {
    let Some(category) = category else {
        return None;
    };
    let category = category.trim();
    if category.is_empty() {
        return None;
    }
    if category.contains('/') || category.contains('\\') {
        return Some(format!(
            "Invalid category '{category}'. Use lowercase letters, numbers, hyphens, dots, and underscores. Categories must be a single directory name."
        ));
    }
    if category.chars().count() > MAX_NAME_LENGTH {
        return Some(format!("Category exceeds {MAX_NAME_LENGTH} characters."));
    }
    if !Regex::new(r"^[a-z0-9][a-z0-9._-]*$")
        .expect("category regex compiles")
        .is_match(category)
    {
        return Some(format!(
            "Invalid category '{category}'. Use lowercase letters, numbers, hyphens, dots, and underscores. Categories must be a single directory name."
        ));
    }
    None
}

fn validate_frontmatter(content: &str) -> Option<String> {
    if content.trim().is_empty() {
        return Some("Content cannot be empty.".to_string());
    }
    if !content.starts_with("---") {
        return Some(
            "SKILL.md must start with YAML frontmatter (---). See existing skills for format."
                .to_string(),
        );
    }
    let rest = &content[3..];
    let Some(end) = rest.find("\n---") else {
        return Some(
            "SKILL.md frontmatter is not closed. Ensure you have a closing '---' line.".to_string(),
        );
    };
    let yaml = &rest[..end];
    let body_start = end + "\n---".len();
    let body = rest
        .get(body_start..)
        .unwrap_or("")
        .trim_start_matches(['\r', '\n'])
        .trim();
    if yaml.lines().any(|line| line.trim_start().starts_with(':')) {
        return Some("YAML frontmatter parse error: invalid mapping key".to_string());
    }
    let keys = yaml
        .lines()
        .filter_map(|line| {
            line.split_once(':')
                .map(|(key, value)| (key.trim(), value.trim()))
        })
        .collect::<Vec<_>>();
    if keys.iter().all(|(key, _)| *key != "name") {
        return Some("Frontmatter must include 'name' field.".to_string());
    }
    let Some((_, description)) = keys.iter().find(|(key, _)| *key == "description") else {
        return Some("Frontmatter must include 'description' field.".to_string());
    };
    if description.chars().count() > MAX_DESCRIPTION_LENGTH {
        return Some(format!(
            "Description exceeds {MAX_DESCRIPTION_LENGTH} characters."
        ));
    }
    if body.is_empty() {
        return Some(
            "SKILL.md must have content after the frontmatter (instructions, procedures, etc.)."
                .to_string(),
        );
    }
    None
}

fn validate_content_size(content: &str, label: &str) -> Option<String> {
    let count = content.chars().count();
    if count > MAX_SKILL_CONTENT_CHARS {
        return Some(format!(
            "{label} content is {} characters (limit: {}). Consider splitting into a smaller SKILL.md with supporting files in references/ or templates/.",
            format_count(count),
            format_count(MAX_SKILL_CONTENT_CHARS)
        ));
    }
    None
}

fn validate_file_path(file_path: &str) -> Option<String> {
    if file_path.is_empty() {
        return Some("file_path is required.".to_string());
    }
    if has_traversal_component(file_path) {
        return Some("Path traversal ('..') is not allowed.".to_string());
    }
    let path = Path::new(file_path);
    let parts = path.components().collect::<Vec<_>>();
    let Some(Component::Normal(first)) = parts.first() else {
        return Some(format!(
            "File must be under one of: assets, references, scripts, templates. Got: '{file_path}'"
        ));
    };
    let first = first.to_string_lossy();
    if !ALLOWED_SUBDIRS.contains(&first.as_ref()) {
        return Some(format!(
            "File must be under one of: assets, references, scripts, templates. Got: '{file_path}'"
        ));
    }
    if parts.len() < 2 {
        return Some(format!(
            "Provide a file path, not just a directory. Example: '{first}/myfile.md'"
        ));
    }
    None
}

fn resolve_skill_dir(skills_dir: &Path, name: &str, category: Option<&str>) -> PathBuf {
    if let Some(category) = category.map(str::trim).filter(|value| !value.is_empty()) {
        skills_dir.join(category).join(name)
    } else {
        skills_dir.join(name)
    }
}

fn find_skill(skills_dir: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    if !skills_dir.exists() {
        return Ok(None);
    }
    for skill_md in find_skill_files(skills_dir)? {
        if skill_md
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|file_name| file_name.to_str())
            == Some(name)
        {
            return Ok(skill_md.parent().map(Path::to_path_buf));
        }
    }
    Ok(None)
}

fn find_skill_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
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

fn resolve_skill_target(skill_dir: &Path, file_path: &str) -> Option<PathBuf> {
    let target = skill_dir.join(file_path);
    let parent = target.parent()?;
    let resolved_parent = if parent.exists() {
        parent.canonicalize().ok()?
    } else {
        existing_ancestor(parent)?.canonicalize().ok()?
    };
    let resolved_skill = skill_dir.canonicalize().ok()?;
    if resolved_parent.starts_with(resolved_skill) {
        Some(target)
    } else {
        None
    }
}

fn existing_ancestor(path: &Path) -> Option<&Path> {
    let mut current = path;
    loop {
        if current.exists() {
            return Some(current);
        }
        current = current.parent()?;
    }
}

fn available_supporting_files(skill_dir: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    for subdir in ALLOWED_SUBDIRS {
        let root = skill_dir.join(subdir);
        if !root.exists() {
            continue;
        }
        collect_files(skill_dir, &root, &mut out)?;
    }
    out.sort();
    Ok(out)
}

fn collect_files(base: &Path, root: &Path, out: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out)?;
        } else if path.is_file() {
            out.push(
                path.strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn has_traversal_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

fn format_count(count: usize) -> String {
    let text = count.to_string();
    let mut out = String::new();
    for (idx, ch) in text.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn tool_error(message: &str) -> Value {
    json!({"error": message, "success": false})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_name_shape() {
        assert_eq!(validate_name("my-skill"), None);
        assert!(validate_name("Bad Skill").is_some());
    }

    #[test]
    fn rejects_path_traversal() {
        assert_eq!(
            validate_file_path("references/../../secret"),
            Some("Path traversal ('..') is not allowed.".to_string())
        );
    }
}

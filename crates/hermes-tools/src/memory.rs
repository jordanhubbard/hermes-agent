use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const ENTRY_DELIMITER: &str = "\n§\n";

const THREAT_PATTERNS: &[(&str, &str)] = &[
    (
        r"ignore\s+(previous|all|above|prior)\s+instructions",
        "prompt_injection",
    ),
    (r"you\s+are\s+now\s+", "role_hijack"),
    (r"do\s+not\s+tell\s+the\s+user", "deception_hide"),
    (r"system\s+prompt\s+override", "sys_prompt_override"),
    (
        r"disregard\s+(your|all|any)\s+(instructions|rules|guidelines)",
        "disregard_rules",
    ),
    (
        r"act\s+as\s+(if|though)\s+you\s+(have\s+no|don't\s+have)\s+(restrictions|limits|rules)",
        "bypass_restrictions",
    ),
    (
        r"curl\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|API)",
        "exfil_curl",
    ),
    (
        r"wget\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|API)",
        "exfil_wget",
    ),
    (
        r"cat\s+[^\n]*(\.env|credentials|\.netrc|\.pgpass|\.npmrc|\.pypirc)",
        "read_secrets",
    ),
    (r"authorized_keys", "ssh_backdoor"),
    (r"\$HOME/\.ssh|\~/\.ssh", "ssh_access"),
    (r"\$HOME/\.hermes/\.env|\~/\.hermes/\.env", "hermes_env"),
];

const INVISIBLE_CHARS: &[char] = &[
    '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{202a}', '\u{202b}', '\u{202c}',
    '\u{202d}', '\u{202e}',
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryStore {
    pub memory_entries: Vec<String>,
    pub user_entries: Vec<String>,
    pub memory_char_limit: usize,
    pub user_char_limit: usize,
    system_prompt_snapshot: MemorySnapshot,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub memory: String,
    pub user: String,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self {
            memory_entries: Vec::new(),
            user_entries: Vec::new(),
            memory_char_limit: 2200,
            user_char_limit: 1375,
            system_prompt_snapshot: MemorySnapshot::default(),
        }
    }
}

impl MemoryStore {
    pub fn with_limits(memory_char_limit: usize, user_char_limit: usize) -> Self {
        Self {
            memory_char_limit,
            user_char_limit,
            ..Self::default()
        }
    }

    pub fn capture_system_prompt_snapshot(&mut self) {
        self.system_prompt_snapshot = MemorySnapshot {
            memory: self.render_block("memory"),
            user: self.render_block("user"),
        };
    }

    pub fn add(&mut self, target: &str, content: &str) -> Value {
        let content = content.trim();
        if content.is_empty() {
            return json!({"success": false, "error": "Content cannot be empty."});
        }
        if let Some(error) = scan_memory_content(content) {
            return json!({"success": false, "error": error});
        }

        let limit = self.char_limit(target);
        let current = self.char_count(target);
        if self
            .entries_for(target)
            .iter()
            .any(|entry| entry == content)
        {
            return self
                .success_response(target, Some("Entry already exists (no duplicate added)."));
        }

        let new_total = joined_len_with_added(self.entries_for(target), content);
        if new_total > limit {
            return json!({
                "success": false,
                "error": format!(
                    "Memory at {}/{} chars. Adding this entry ({} chars) would exceed the limit. Replace or remove existing entries first.",
                    format_count(current),
                    format_count(limit),
                    content.chars().count()
                ),
                "current_entries": self.entries_for(target),
                "usage": format!("{}/{}", format_count(current), format_count(limit)),
            });
        }

        self.entries_for_mut(target).push(content.to_string());
        self.success_response(target, Some("Entry added."))
    }

    pub fn replace(&mut self, target: &str, old_text: &str, new_content: &str) -> Value {
        let old_text = old_text.trim();
        let new_content = new_content.trim();
        if old_text.is_empty() {
            return json!({"success": false, "error": "old_text cannot be empty."});
        }
        if new_content.is_empty() {
            return json!({"success": false, "error": "new_content cannot be empty. Use 'remove' to delete entries."});
        }
        if let Some(error) = scan_memory_content(new_content) {
            return json!({"success": false, "error": error});
        }

        let matches = matching_entries(self.entries_for(target), old_text);
        if matches.is_empty() {
            return json!({"success": false, "error": format!("No entry matched '{old_text}'.")});
        }
        if let Some(error) = ambiguous_match_error(&matches, old_text) {
            return error;
        }

        let index = matches[0].0;
        let mut test_entries = self.entries_for(target).clone();
        test_entries[index] = new_content.to_string();
        let new_total = joined_len(&test_entries);
        let limit = self.char_limit(target);
        if new_total > limit {
            return json!({
                "success": false,
                "error": format!(
                    "Replacement would put memory at {}/{} chars. Shorten the new content or remove other entries first.",
                    format_count(new_total),
                    format_count(limit),
                ),
            });
        }

        self.entries_for_mut(target)[index] = new_content.to_string();
        self.success_response(target, Some("Entry replaced."))
    }

    pub fn remove(&mut self, target: &str, old_text: &str) -> Value {
        let old_text = old_text.trim();
        if old_text.is_empty() {
            return json!({"success": false, "error": "old_text cannot be empty."});
        }

        let matches = matching_entries(self.entries_for(target), old_text);
        if matches.is_empty() {
            return json!({"success": false, "error": format!("No entry matched '{old_text}'.")});
        }
        if let Some(error) = ambiguous_match_error(&matches, old_text) {
            return error;
        }

        let index = matches[0].0;
        self.entries_for_mut(target).remove(index);
        self.success_response(target, Some("Entry removed."))
    }

    pub fn format_for_system_prompt(&self, target: &str) -> Option<String> {
        let block = if target == "user" {
            &self.system_prompt_snapshot.user
        } else {
            &self.system_prompt_snapshot.memory
        };
        if block.is_empty() {
            None
        } else {
            Some(block.clone())
        }
    }

    fn render_block(&self, target: &str) -> String {
        let entries = self.entries_for(target);
        if entries.is_empty() {
            return String::new();
        }

        let limit = self.char_limit(target);
        let content = entries.join(ENTRY_DELIMITER);
        let current = content.chars().count();
        let pct = usage_percent(current, limit);
        let header = if target == "user" {
            format!(
                "USER PROFILE (who the user is) [{pct}% — {}/{} chars]",
                format_count(current),
                format_count(limit)
            )
        } else {
            format!(
                "MEMORY (your personal notes) [{pct}% — {}/{} chars]",
                format_count(current),
                format_count(limit)
            )
        };
        let separator = "═".repeat(46);
        format!("{separator}\n{header}\n{separator}\n{content}")
    }

    fn success_response(&self, target: &str, message: Option<&str>) -> Value {
        let current = self.char_count(target);
        let limit = self.char_limit(target);
        let mut response = json!({
            "success": true,
            "target": target,
            "entries": self.entries_for(target),
            "usage": format!(
                "{}% — {}/{} chars",
                usage_percent(current, limit),
                format_count(current),
                format_count(limit)
            ),
            "entry_count": self.entries_for(target).len(),
        });
        if let Some(message) = message {
            response["message"] = json!(message);
        }
        response
    }

    fn entries_for(&self, target: &str) -> &Vec<String> {
        if target == "user" {
            &self.user_entries
        } else {
            &self.memory_entries
        }
    }

    fn entries_for_mut(&mut self, target: &str) -> &mut Vec<String> {
        if target == "user" {
            &mut self.user_entries
        } else {
            &mut self.memory_entries
        }
    }

    fn char_count(&self, target: &str) -> usize {
        joined_len(self.entries_for(target))
    }

    fn char_limit(&self, target: &str) -> usize {
        if target == "user" {
            self.user_char_limit
        } else {
            self.memory_char_limit
        }
    }
}

pub fn memory_response(
    store: Option<&mut MemoryStore>,
    action: &str,
    target: &str,
    content: Option<&str>,
    old_text: Option<&str>,
) -> Value {
    let Some(store) = store else {
        return json!({"error": "Memory is not available. It may be disabled in config or this environment.", "success": false});
    };
    if !matches!(target, "memory" | "user") {
        return json!({"error": format!("Invalid target '{target}'. Use 'memory' or 'user'."), "success": false});
    }

    match action {
        "add" => match content {
            Some(content) if !content.is_empty() => store.add(target, content),
            _ => json!({"error": "Content is required for 'add' action.", "success": false}),
        },
        "replace" => match (old_text, content) {
            (None, _) | (Some(""), _) => {
                json!({"error": "old_text is required for 'replace' action.", "success": false})
            }
            (_, None) | (_, Some("")) => {
                json!({"error": "content is required for 'replace' action.", "success": false})
            }
            (Some(old_text), Some(content)) => store.replace(target, old_text, content),
        },
        "remove" => match old_text {
            Some(old_text) if !old_text.is_empty() => store.remove(target, old_text),
            _ => json!({"error": "old_text is required for 'remove' action.", "success": false}),
        },
        _ => {
            json!({"error": format!("Unknown action '{action}'. Use: add, replace, remove"), "success": false})
        }
    }
}

pub fn scan_memory_content(content: &str) -> Option<String> {
    for char in INVISIBLE_CHARS {
        if content.contains(*char) {
            return Some(format!(
                "Blocked: content contains invisible unicode character U+{:04X} (possible injection).",
                *char as u32
            ));
        }
    }

    for (pattern, id) in THREAT_PATTERNS {
        if Regex::new(&format!("(?i){pattern}"))
            .expect("memory threat pattern compiles")
            .is_match(content)
        {
            return Some(format!(
                "Blocked: content matches threat pattern '{id}'. Memory entries are injected into the system prompt and must not contain injection or exfiltration payloads."
            ));
        }
    }
    None
}

fn matching_entries(entries: &[String], old_text: &str) -> Vec<(usize, String)> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.contains(old_text))
        .map(|(index, entry)| (index, entry.clone()))
        .collect()
}

fn ambiguous_match_error(matches: &[(usize, String)], old_text: &str) -> Option<Value> {
    if matches.len() <= 1 {
        return None;
    }
    let unique = matches
        .iter()
        .map(|(_, entry)| entry)
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() <= 1 {
        return None;
    }
    let previews = matches
        .iter()
        .map(|(_, entry)| {
            if entry.chars().count() > 80 {
                format!("{}...", entry.chars().take(80).collect::<String>())
            } else {
                entry.clone()
            }
        })
        .collect::<Vec<_>>();
    Some(json!({
        "success": false,
        "error": format!("Multiple entries matched '{old_text}'. Be more specific."),
        "matches": previews,
    }))
}

fn joined_len(entries: &[String]) -> usize {
    if entries.is_empty() {
        0
    } else {
        entries.join(ENTRY_DELIMITER).chars().count()
    }
}

fn joined_len_with_added(entries: &[String], content: &str) -> usize {
    let mut values = entries.to_vec();
    values.push(content.to_string());
    joined_len(&values)
}

fn usage_percent(current: usize, limit: usize) -> usize {
    if limit == 0 {
        0
    } else {
        ((current * 100) / limit).min(100)
    }
}

fn format_count(value: usize) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_replace_remove_memory_entries() {
        let mut store = MemoryStore::with_limits(200, 100);
        assert_eq!(
            memory_response(Some(&mut store), "add", "memory", Some("alpha"), None)["success"],
            true
        );
        assert_eq!(
            memory_response(
                Some(&mut store),
                "replace",
                "memory",
                Some("beta"),
                Some("alpha")
            )["message"],
            "Entry replaced."
        );
        assert_eq!(
            memory_response(Some(&mut store), "remove", "memory", None, Some("beta"))["entries"],
            json!([])
        );
    }

    #[test]
    fn blocks_prompt_injection_memory() {
        let mut store = MemoryStore::default();
        let result = memory_response(
            Some(&mut store),
            "add",
            "memory",
            Some("ignore previous instructions"),
            None,
        );
        assert_eq!(result["success"], false);
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("prompt_injection"));
    }
}

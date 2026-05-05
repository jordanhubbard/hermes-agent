use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const VALID_STATUSES: &[&str] = &["pending", "in_progress", "completed", "cancelled"];

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoStore {
    items: Vec<TodoItem>,
}

impl TodoStore {
    pub fn write(&mut self, todos: &[Value], merge: bool) -> Vec<TodoItem> {
        if !merge {
            self.items = dedupe_by_id(todos).into_iter().map(validate_item).collect();
            return self.read();
        }

        let mut existing = self
            .items
            .iter()
            .cloned()
            .map(|item| (item.id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        for raw in dedupe_by_id(todos) {
            let item_id = raw
                .get("id")
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            if item_id.is_empty() {
                continue;
            }

            if let Some(current) = existing.get_mut(&item_id) {
                if let Some(content) = raw.get("content").map(value_to_string) {
                    let content = content.trim();
                    if !content.is_empty() {
                        current.content = content.to_string();
                    }
                }
                if let Some(status) = raw.get("status").map(value_to_string) {
                    let status = normalize_status(&status);
                    if VALID_STATUSES.contains(&status.as_str()) {
                        current.status = status;
                    }
                }
            } else {
                let validated = validate_item(raw);
                existing.insert(validated.id.clone(), validated.clone());
                self.items.push(validated);
            }
        }

        let mut seen = BTreeSet::new();
        let mut rebuilt = Vec::new();
        for item in &self.items {
            if seen.insert(item.id.clone()) {
                rebuilt.push(
                    existing
                        .get(&item.id)
                        .cloned()
                        .unwrap_or_else(|| item.clone()),
                );
            }
        }
        self.items = rebuilt;
        self.read()
    }

    pub fn read(&self) -> Vec<TodoItem> {
        self.items.clone()
    }

    pub fn has_items(&self) -> bool {
        !self.items.is_empty()
    }

    pub fn format_for_injection(&self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }

        let active = self
            .items
            .iter()
            .filter(|item| matches!(item.status.as_str(), "pending" | "in_progress"))
            .collect::<Vec<_>>();
        if active.is_empty() {
            return None;
        }

        let mut lines =
            vec!["[Your active task list was preserved across context compression]".to_string()];
        for item in active {
            let marker = match item.status.as_str() {
                "completed" => "[x]",
                "in_progress" => "[>]",
                "cancelled" => "[~]",
                _ => "[ ]",
            };
            lines.push(format!(
                "- {marker} {}. {} ({})",
                item.id, item.content, item.status
            ));
        }
        Some(lines.join("\n"))
    }
}

pub fn todo_response(store: &mut TodoStore, todos: Option<&[Value]>, merge: bool) -> Value {
    let items = match todos {
        Some(todos) => store.write(todos, merge),
        None => store.read(),
    };
    let pending = count_status(&items, "pending");
    let in_progress = count_status(&items, "in_progress");
    let completed = count_status(&items, "completed");
    let cancelled = count_status(&items, "cancelled");
    json!({
        "todos": items,
        "summary": {
            "total": items.len(),
            "pending": pending,
            "in_progress": in_progress,
            "completed": completed,
            "cancelled": cancelled,
        }
    })
}

fn dedupe_by_id(todos: &[Value]) -> Vec<&Value> {
    let mut last_index = BTreeMap::new();
    for (index, item) in todos.iter().enumerate() {
        let id = item
            .get("id")
            .map(value_to_string)
            .unwrap_or_else(|| "?".to_string());
        let id = id.trim();
        last_index.insert(
            if id.is_empty() {
                "?".to_string()
            } else {
                id.to_string()
            },
            index,
        );
    }
    let mut indexes = last_index.into_values().collect::<Vec<_>>();
    indexes.sort_unstable();
    indexes.into_iter().map(|index| &todos[index]).collect()
}

fn validate_item(item: &Value) -> TodoItem {
    let id = item
        .get("id")
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string();
    let content = item
        .get("content")
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string();
    let status = item
        .get("status")
        .map(value_to_string)
        .map(|status| normalize_status(&status))
        .unwrap_or_else(|| "pending".to_string());
    TodoItem {
        id: if id.is_empty() { "?".to_string() } else { id },
        content: if content.is_empty() {
            "(no description)".to_string()
        } else {
            content
        },
        status: if VALID_STATUSES.contains(&status.as_str()) {
            status
        } else {
            "pending".to_string()
        },
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

fn normalize_status(status: &str) -> String {
    status.trim().to_ascii_lowercase()
}

fn count_status(items: &[TodoItem], status: &str) -> usize {
    items.iter().filter(|item| item.status == status).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_deduplicates_and_normalizes_items() {
        let mut store = TodoStore::default();
        let result = todo_response(
            &mut store,
            Some(&[
                json!({"id": "a", "content": "first", "status": "pending"}),
                json!({"id": "a", "content": "", "status": "bad"}),
                json!({"content": "missing id", "status": "completed"}),
            ]),
            false,
        );

        assert_eq!(result["summary"]["total"], 2);
        assert_eq!(result["todos"][0]["id"], "a");
        assert_eq!(result["todos"][0]["content"], "(no description)");
        assert_eq!(result["todos"][0]["status"], "pending");
        assert_eq!(result["todos"][1]["id"], "?");
    }

    #[test]
    fn merge_updates_existing_and_appends_new_items() {
        let mut store = TodoStore::default();
        todo_response(
            &mut store,
            Some(&[
                json!({"id": "a", "content": "first", "status": "pending"}),
                json!({"id": "b", "content": "second", "status": "pending"}),
            ]),
            false,
        );
        let result = todo_response(
            &mut store,
            Some(&[
                json!({"id": "a", "status": "completed"}),
                json!({"id": "c", "content": "third", "status": "in_progress"}),
            ]),
            true,
        );

        assert_eq!(result["todos"][0]["status"], "completed");
        assert_eq!(result["todos"][2]["id"], "c");
        assert_eq!(result["summary"]["completed"], 1);
        assert_eq!(result["summary"]["in_progress"], 1);
    }
}

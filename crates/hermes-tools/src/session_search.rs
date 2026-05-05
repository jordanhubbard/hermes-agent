use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SESSION_CHARS: usize = 100_000;
const HIDDEN_SESSION_SOURCES: &[&str] = &["tool"];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionSearchStore {
    pub sessions: BTreeMap<String, SessionRecord>,
    pub recent_sessions: Vec<SessionRecord>,
    pub messages: BTreeMap<String, Vec<ConversationMessage>>,
    pub search_results: Vec<SearchMatch>,
}

impl SessionSearchStore {
    pub fn get_session(&self, session_id: &str) -> Option<&SessionRecord> {
        self.sessions.get(session_id)
    }

    pub fn list_recent_sessions(&self, limit: usize) -> Vec<SessionRecord> {
        self.recent_sessions
            .iter()
            .filter(|session| !HIDDEN_SESSION_SOURCES.contains(&session.source.as_str()))
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn search_messages(
        &self,
        _query: &str,
        role_filter: Option<&[String]>,
    ) -> Vec<SearchMatch> {
        self.search_results
            .iter()
            .filter(|result| !HIDDEN_SESSION_SOURCES.contains(&result.source.as_str()))
            .filter(|result| {
                role_filter
                    .map(|roles| roles.iter().any(|role| role == &result.role))
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn get_messages_as_conversation(&self, session_id: &str) -> Vec<ConversationMessage> {
        self.messages.get(session_id).cloned().unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionRecord {
    pub id: String,
    pub title: Option<String>,
    pub source: String,
    pub started_at: Option<Value>,
    pub last_active: Option<Value>,
    pub message_count: i64,
    pub preview: String,
    pub parent_session_id: Option<String>,
    pub model: Option<String>,
}

impl SessionRecord {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchMatch {
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub source: String,
    pub session_started: Option<Value>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConversationMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_name: Option<String>,
    pub tool_calls: Option<Value>,
}

pub trait SessionSummarizer {
    fn summarize(
        &self,
        conversation_text: &str,
        query: &str,
        session_meta: &SessionRecord,
    ) -> Option<String>;
}

pub fn session_search_response(
    db: Option<&SessionSearchStore>,
    query: &str,
    role_filter: Option<&str>,
    limit: &Value,
    current_session_id: Option<&str>,
    summarizer: Option<&dyn SessionSummarizer>,
) -> Value {
    let Some(db) = db else {
        return json!({"error": "Session database not available.", "success": false});
    };

    let limit = coerce_limit(limit);
    if query.trim().is_empty() {
        return list_recent_sessions(db, limit, current_session_id);
    }

    let query = query.trim();
    let role_list = parse_role_filter(role_filter);
    let raw_results = db.search_messages(query, role_list.as_deref());
    if raw_results.is_empty() {
        return json!({
            "success": true,
            "query": query,
            "results": [],
            "count": 0,
            "message": "No matching sessions found.",
        });
    }

    let current_lineage_root =
        current_session_id.map(|session_id| resolve_to_parent(db, session_id));
    let mut seen_ids = BTreeSet::new();
    let mut seen_sessions = Vec::new();
    for result in raw_results {
        let raw_sid = result.session_id.clone();
        let resolved_sid = resolve_to_parent(db, &raw_sid);
        if current_lineage_root.as_deref() == Some(resolved_sid.as_str()) {
            continue;
        }
        if current_session_id == Some(raw_sid.as_str()) {
            continue;
        }
        if seen_ids.insert(resolved_sid.clone()) {
            let mut result = result;
            result.session_id = resolved_sid.clone();
            seen_sessions.push((resolved_sid, result));
        }
        if seen_sessions.len() >= limit {
            break;
        }
    }

    let mut summaries = Vec::new();
    for (session_id, match_info) in &seen_sessions {
        let messages = db.get_messages_as_conversation(session_id);
        if messages.is_empty() {
            continue;
        }
        let session_meta = db.get_session(session_id).cloned().unwrap_or_else(|| {
            let mut record = SessionRecord::new(session_id);
            record.source = match_info.source.clone();
            record.started_at = match_info.session_started.clone();
            record.model = match_info.model.clone();
            record
        });
        let conversation_text =
            truncate_around_matches(&format_conversation(&messages), query, MAX_SESSION_CHARS);
        let summary = summarizer
            .and_then(|summarizer| summarizer.summarize(&conversation_text, query, &session_meta))
            .unwrap_or_else(|| raw_preview_summary(&conversation_text));

        summaries.push(json!({
            "session_id": session_id,
            "when": format_timestamp(session_meta.started_at.as_ref().or(match_info.session_started.as_ref())),
            "source": first_non_empty(&session_meta.source, &match_info.source, "unknown"),
            "model": session_meta.model.clone().or_else(|| match_info.model.clone()),
            "summary": summary,
        }));
    }

    json!({
        "success": true,
        "query": query,
        "results": summaries,
        "count": summaries.len(),
        "sessions_searched": seen_sessions.len(),
    })
}

fn list_recent_sessions(
    db: &SessionSearchStore,
    limit: usize,
    current_session_id: Option<&str>,
) -> Value {
    let current_root = current_session_id.map(|session_id| resolve_to_parent(db, session_id));
    let mut results = Vec::new();
    for session in db.list_recent_sessions(limit + 5) {
        if current_root.as_deref() == Some(session.id.as_str())
            || current_session_id == Some(session.id.as_str())
        {
            continue;
        }
        if session.parent_session_id.is_some() {
            continue;
        }
        results.push(json!({
            "session_id": session.id,
            "title": session.title,
            "source": session.source,
            "started_at": session.started_at.unwrap_or_else(|| json!("")),
            "last_active": session.last_active.unwrap_or_else(|| json!("")),
            "message_count": session.message_count,
            "preview": session.preview,
        }));
        if results.len() >= limit {
            break;
        }
    }

    json!({
        "success": true,
        "mode": "recent",
        "results": results,
        "count": results.len(),
        "message": format!(
            "Showing {} most recent sessions. Use a keyword query to search specific topics.",
            results.len()
        ),
    })
}

fn coerce_limit(value: &Value) -> usize {
    let parsed = match value {
        Value::Number(number) => number.as_i64().unwrap_or(3),
        Value::String(text) => text.parse::<i64>().unwrap_or(3),
        _ => 3,
    };
    parsed.clamp(1, 5) as usize
}

fn parse_role_filter(role_filter: Option<&str>) -> Option<Vec<String>> {
    let roles = role_filter?
        .split(',')
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if roles.is_empty() {
        None
    } else {
        Some(roles)
    }
}

fn resolve_to_parent(db: &SessionSearchStore, session_id: &str) -> String {
    let mut sid = session_id.to_string();
    let mut visited = BTreeSet::new();
    while !sid.is_empty() && visited.insert(sid.clone()) {
        let Some(session) = db.get_session(&sid) else {
            break;
        };
        let Some(parent) = session.parent_session_id.as_ref().filter(|p| !p.is_empty()) else {
            break;
        };
        sid = parent.clone();
    }
    sid
}

fn format_timestamp(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "unknown".to_string(),
        Some(Value::String(text)) if text.is_empty() => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

fn first_non_empty(primary: &str, fallback: &str, default: &str) -> String {
    if !primary.is_empty() {
        primary.to_string()
    } else if !fallback.is_empty() {
        fallback.to_string()
    } else {
        default.to_string()
    }
}

pub fn format_conversation(messages: &[ConversationMessage]) -> String {
    messages
        .iter()
        .map(format_message)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_message(message: &ConversationMessage) -> String {
    let role = message.role.to_ascii_uppercase();
    let mut content = message.content.clone().unwrap_or_default();
    if role == "TOOL" {
        if let Some(tool_name) = message.tool_name.as_ref() {
            if content.chars().count() > 500 {
                content = format!(
                    "{}\n...[truncated]...\n{}",
                    take_chars(&content, 250),
                    take_last_chars(&content, 250)
                );
            }
            return format!("[TOOL:{tool_name}]: {content}");
        }
    }

    if role == "ASSISTANT" {
        if let Some(tool_calls) = message.tool_calls.as_ref().and_then(Value::as_array) {
            let names = tool_calls
                .iter()
                .filter_map(tool_call_name)
                .collect::<Vec<_>>();
            let mut parts = Vec::new();
            if !names.is_empty() {
                parts.push(format!("[ASSISTANT]: [Called: {}]", names.join(", ")));
            }
            if !content.is_empty() {
                parts.push(format!("[ASSISTANT]: {content}"));
            }
            return parts.join("\n\n");
        }
    }

    format!("[{role}]: {content}")
}

fn tool_call_name(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    obj.get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            obj.get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string)
}

fn truncate_around_matches(full_text: &str, query: &str, max_chars: usize) -> String {
    if full_text.chars().count() <= max_chars {
        return full_text.to_string();
    }
    let text_lower = full_text.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let match_pos = text_lower
        .find(query_lower.trim())
        .unwrap_or(0)
        .saturating_sub(max_chars / 4);
    let start = char_floor(full_text, match_pos);
    let end = char_ceil(full_text, start.saturating_add(max_chars));
    let mut truncated = full_text[start..end.min(full_text.len())].to_string();
    if start > 0 {
        truncated = format!("...[earlier conversation truncated]...\n\n{truncated}");
    }
    if end < full_text.len() {
        truncated.push_str("\n\n...[later conversation truncated]...");
    }
    truncated
}

fn raw_preview_summary(conversation_text: &str) -> String {
    let preview = if conversation_text.is_empty() {
        "No preview available.".to_string()
    } else {
        format!("{}\n…[truncated]", take_chars(conversation_text, 500))
    };
    format!("[Raw preview — summarization unavailable]\n{preview}")
}

fn take_chars(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn take_last_chars(value: &str, count: usize) -> String {
    let mut chars = value.chars().rev().take(count).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

fn char_floor(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    value
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|idx| *idx <= index)
        .last()
        .unwrap_or(0)
}

fn char_ceil(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }
    value
        .char_indices()
        .map(|(idx, _)| idx)
        .find(|idx| *idx >= index)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_current_lineage() {
        let mut store = SessionSearchStore::default();
        store.sessions.insert(
            "child".to_string(),
            SessionRecord {
                id: "child".to_string(),
                parent_session_id: Some("root".to_string()),
                ..SessionRecord::default()
            },
        );
        store
            .sessions
            .insert("root".to_string(), SessionRecord::new("root"));
        store.search_results.push(SearchMatch {
            session_id: "root".to_string(),
            role: "user".to_string(),
            source: "cli".to_string(),
            ..SearchMatch::default()
        });

        let result =
            session_search_response(Some(&store), "test", None, &json!(3), Some("child"), None);
        assert_eq!(result["sessions_searched"], 0);
    }
}

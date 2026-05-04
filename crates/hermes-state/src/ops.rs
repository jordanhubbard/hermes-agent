//! Shared op dispatcher used by the JSON probe and the long-running daemon.
//!
//! The probe reads a JSON array of ops from stdin and writes results to
//! stdout. The daemon reads framed JSON requests over a Unix socket and
//! writes framed responses. Both call [`run_operation`] for the actual
//! work so they cannot drift.

use crate::{
    AppendMessage, ConversationMessage, ExportedSession, NewSession, SearchContextMessage,
    SearchMatch, SearchOptions, SessionListOptions, SessionListRecord, SessionRecord,
    SessionRichOptions, SessionRichRecord, SessionStore, StoredMessage, TokenUpdate,
};
use serde_json::{json, Value};

/// Apply a single operation against a [`SessionStore`].
///
/// Errors are stringified for transport — the wire protocol carries
/// `Result<Value, String>` rather than typed Rust errors.
pub fn run_operation(store: &mut SessionStore, operation: Value) -> Result<Value, String> {
    let op = operation
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| "operation missing string field `op`".to_string())?;
    match op {
        "schema_version" => Ok(json!(store
            .schema_version()
            .map_err(|err| err.to_string())?)),
        "create_session" => {
            let id = required_str(&operation, "id")?;
            let source = required_str(&operation, "source")?;
            let mut session = NewSession::new(id, source);
            session.user_id = optional_string(&operation, "user_id");
            session.model = optional_string(&operation, "model");
            session.model_config = operation.get("model_config").cloned();
            session.system_prompt = optional_string(&operation, "system_prompt");
            session.parent_session_id = optional_string(&operation, "parent_session_id");
            Ok(json!(store
                .create_session(session)
                .map_err(|err| err.to_string())?))
        }
        "get_session" => {
            let id = required_str(&operation, "id")?;
            Ok(
                match store.get_session(id).map_err(|err| err.to_string())? {
                    Some(session) => session_to_json(session),
                    None => Value::Null,
                },
            )
        }
        "end_session" => {
            let id = required_str(&operation, "id")?;
            let end_reason = required_str(&operation, "end_reason")?;
            store
                .end_session(id, end_reason)
                .map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "reopen_session" => {
            let id = required_str(&operation, "id")?;
            store.reopen_session(id).map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "update_system_prompt" => {
            let id = required_str(&operation, "id")?;
            let system_prompt = required_str(&operation, "system_prompt")?;
            store
                .update_system_prompt(id, system_prompt)
                .map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "set_session_title" => {
            let id = required_str(&operation, "id")?;
            let title = operation.get("title").and_then(Value::as_str);
            Ok(json!(store
                .set_session_title(id, title)
                .map_err(|err| err.to_string())?))
        }
        "get_session_title" => {
            let id = required_str(&operation, "id")?;
            Ok(json!(store
                .get_session_title(id)
                .map_err(|err| err.to_string())?))
        }
        "get_session_by_title" => {
            let title = required_str(&operation, "title")?;
            Ok(
                match store
                    .get_session_by_title(title)
                    .map_err(|err| err.to_string())?
                {
                    Some(session) => session_to_json(session),
                    None => Value::Null,
                },
            )
        }
        "resolve_session_by_title" => {
            let title = required_str(&operation, "title")?;
            Ok(json!(store
                .resolve_session_by_title(title)
                .map_err(|err| err.to_string())?))
        }
        "get_next_title_in_lineage" => {
            let title = required_str(&operation, "title")?;
            Ok(json!(store
                .get_next_title_in_lineage(title)
                .map_err(|err| err.to_string())?))
        }
        "update_token_counts" => {
            let id = required_str(&operation, "id")?;
            store
                .update_token_counts(id, token_update_from_json(&operation))
                .map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "append_message" => {
            let message = append_message_from_json(&operation);
            Ok(json!(store
                .append_message(message)
                .map_err(|err| err.to_string())?))
        }
        "replace_messages" => {
            let session_id = required_str(&operation, "session_id")?;
            let raw_messages = operation
                .get("messages")
                .and_then(Value::as_array)
                .ok_or_else(|| "operation missing array field `messages`".to_string())?;
            let messages = raw_messages
                .iter()
                .map(append_message_from_json)
                .collect::<Vec<_>>();
            store
                .replace_messages(session_id, &messages)
                .map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "get_messages" => {
            let session_id = required_str(&operation, "session_id")?;
            let messages = store
                .get_messages(session_id)
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                messages.into_iter().map(stored_message_to_json).collect(),
            ))
        }
        "get_messages_as_conversation" => {
            let session_id = required_str(&operation, "session_id")?;
            let include_ancestors = operation
                .get("include_ancestors")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let messages = store
                .get_messages_as_conversation(session_id, include_ancestors)
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                messages
                    .into_iter()
                    .map(conversation_message_to_json)
                    .collect(),
            ))
        }
        "search_messages" => {
            let query = required_str(&operation, "query")?;
            let options = search_options_from_json(&operation);
            let matches = store
                .search_messages(query, options)
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                matches.into_iter().map(search_match_to_json).collect(),
            ))
        }
        "search_sessions" => {
            let sessions = store
                .search_sessions(session_list_options_from_json(&operation))
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                sessions.into_iter().map(session_list_to_json).collect(),
            ))
        }
        "list_sessions_rich" => {
            let sessions = store
                .list_sessions_rich(session_rich_options_from_json(&operation))
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                sessions.into_iter().map(session_rich_to_json).collect(),
            ))
        }
        "session_count" => Ok(json!(store
            .session_count(optional_string(&operation, "source").as_deref())
            .map_err(|err| err.to_string())?)),
        "message_count" => Ok(json!(store
            .message_count(optional_string(&operation, "session_id").as_deref())
            .map_err(|err| err.to_string())?)),
        "export_session" => {
            let session_id = required_str(&operation, "session_id")?;
            Ok(
                match store
                    .export_session(session_id)
                    .map_err(|err| err.to_string())?
                {
                    Some(exported) => exported_session_to_json(exported),
                    None => Value::Null,
                },
            )
        }
        "export_all" => {
            let exports = store
                .export_all(optional_string(&operation, "source").as_deref())
                .map_err(|err| err.to_string())?;
            Ok(Value::Array(
                exports.into_iter().map(exported_session_to_json).collect(),
            ))
        }
        "clear_messages" => {
            let session_id = required_str(&operation, "session_id")?;
            store
                .clear_messages(session_id)
                .map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "delete_session" => {
            let session_id = required_str(&operation, "session_id")?;
            Ok(json!(store
                .delete_session(session_id)
                .map_err(|err| err.to_string())?))
        }
        "resolve_session_id" => {
            let session_id_or_prefix = required_str(&operation, "session_id_or_prefix")?;
            Ok(json!(store
                .resolve_session_id(session_id_or_prefix)
                .map_err(|err| err.to_string())?))
        }
        "resolve_resume_session_id" => {
            let session_id = required_str(&operation, "session_id")?;
            Ok(json!(store
                .resolve_resume_session_id(session_id)
                .map_err(|err| err.to_string())?))
        }
        "get_compression_tip" => {
            let session_id = required_str(&operation, "session_id")?;
            Ok(json!(store
                .get_compression_tip(session_id)
                .map_err(|err| err.to_string())?))
        }
        "prune_empty_ghost_sessions" => Ok(json!(store
            .prune_empty_ghost_sessions()
            .map_err(|err| err.to_string())?)),
        "prune_sessions" => {
            let older_than_days = optional_i64(&operation, "older_than_days").unwrap_or(90);
            let ids = store
                .prune_sessions(
                    older_than_days,
                    optional_string(&operation, "source").as_deref(),
                )
                .map_err(|err| err.to_string())?;
            Ok(json!(ids))
        }
        "get_meta" => {
            let key = required_str(&operation, "key")?;
            Ok(json!(store.get_meta(key).map_err(|err| err.to_string())?))
        }
        "set_meta" => {
            let key = required_str(&operation, "key")?;
            let value = required_str(&operation, "value")?;
            store.set_meta(key, value).map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        "vacuum" => {
            store.vacuum().map_err(|err| err.to_string())?;
            Ok(Value::Null)
        }
        _ => Err(format!("unknown operation: {op}")),
    }
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("operation missing string field `{key}`"))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn optional_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn optional_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

fn optional_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn optional_json_value(value: &Value, key: &str) -> Option<Value> {
    match value.get(key) {
        Some(Value::Null) | None => None,
        Some(raw) => Some(raw.clone()),
    }
}

fn optional_string_array(value: &Value, key: &str) -> Option<Vec<String>> {
    value.get(key).and_then(|raw| {
        raw.as_array().map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
    })
}

fn append_message_from_json(value: &Value) -> AppendMessage {
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let mut message = AppendMessage::new(session_id, role);
    message.content = optional_json_value(value, "content");
    message.tool_name = optional_string(value, "tool_name");
    message.tool_calls = optional_json_value(value, "tool_calls");
    message.tool_call_id = optional_string(value, "tool_call_id");
    message.token_count = optional_i64(value, "token_count");
    message.finish_reason = optional_string(value, "finish_reason");
    message.reasoning = optional_string(value, "reasoning");
    message.reasoning_content = optional_string(value, "reasoning_content");
    message.reasoning_details = optional_json_value(value, "reasoning_details");
    message.codex_reasoning_items = optional_json_value(value, "codex_reasoning_items");
    message.codex_message_items = optional_json_value(value, "codex_message_items");
    message
}

fn token_update_from_json(value: &Value) -> TokenUpdate {
    TokenUpdate {
        input_tokens: optional_i64(value, "input_tokens").unwrap_or(0),
        output_tokens: optional_i64(value, "output_tokens").unwrap_or(0),
        cache_read_tokens: optional_i64(value, "cache_read_tokens").unwrap_or(0),
        cache_write_tokens: optional_i64(value, "cache_write_tokens").unwrap_or(0),
        reasoning_tokens: optional_i64(value, "reasoning_tokens").unwrap_or(0),
        estimated_cost_usd: optional_f64(value, "estimated_cost_usd"),
        actual_cost_usd: optional_f64(value, "actual_cost_usd"),
        cost_status: optional_string(value, "cost_status"),
        cost_source: optional_string(value, "cost_source"),
        pricing_version: optional_string(value, "pricing_version"),
        billing_provider: optional_string(value, "billing_provider"),
        billing_base_url: optional_string(value, "billing_base_url"),
        billing_mode: optional_string(value, "billing_mode"),
        model: optional_string(value, "model"),
        api_call_count: optional_i64(value, "api_call_count").unwrap_or(0),
        absolute: optional_bool(value, "absolute").unwrap_or(false),
    }
}

fn search_options_from_json(value: &Value) -> SearchOptions {
    SearchOptions {
        source_filter: optional_string_array(value, "source_filter"),
        exclude_sources: optional_string_array(value, "exclude_sources"),
        role_filter: optional_string_array(value, "role_filter"),
        limit: optional_i64(value, "limit").unwrap_or(20),
        offset: optional_i64(value, "offset").unwrap_or(0),
    }
}

fn session_list_options_from_json(value: &Value) -> SessionListOptions {
    SessionListOptions {
        source: optional_string(value, "source"),
        limit: optional_i64(value, "limit").unwrap_or(20),
        offset: optional_i64(value, "offset").unwrap_or(0),
    }
}

fn session_rich_options_from_json(value: &Value) -> SessionRichOptions {
    SessionRichOptions {
        source: optional_string(value, "source"),
        exclude_sources: optional_string_array(value, "exclude_sources"),
        limit: optional_i64(value, "limit").unwrap_or(20),
        offset: optional_i64(value, "offset").unwrap_or(0),
        include_children: optional_bool(value, "include_children").unwrap_or(false),
        project_compression_tips: optional_bool(value, "project_compression_tips").unwrap_or(true),
        order_by_last_active: optional_bool(value, "order_by_last_active").unwrap_or(false),
    }
}

fn stored_message_to_json(message: StoredMessage) -> Value {
    json!({
        "id": message.id,
        "session_id": message.session_id,
        "role": message.role,
        "content": message.content,
        "tool_call_id": message.tool_call_id,
        "tool_calls": message.tool_calls,
        "tool_name": message.tool_name,
        "timestamp": message.timestamp,
        "token_count": message.token_count,
        "finish_reason": message.finish_reason,
        "reasoning": message.reasoning,
        "reasoning_content": message.reasoning_content,
        "reasoning_details": message.reasoning_details,
        "codex_reasoning_items": message.codex_reasoning_items,
        "codex_message_items": message.codex_message_items,
    })
}

fn session_list_to_json(row: SessionListRecord) -> Value {
    let mut value = session_to_json(row.session);
    if let Value::Object(object) = &mut value {
        object.insert("last_active".to_string(), json!(row.last_active));
    }
    value
}

fn session_rich_to_json(row: SessionRichRecord) -> Value {
    let mut value = session_to_json(row.session);
    if let Value::Object(object) = &mut value {
        object.insert("preview".to_string(), json!(row.preview));
        object.insert("last_active".to_string(), json!(row.last_active));
        if let Some(root_id) = row.lineage_root_id {
            object.insert("_lineage_root_id".to_string(), json!(root_id));
        }
    }
    value
}

fn exported_session_to_json(exported: ExportedSession) -> Value {
    let mut value = session_to_json(exported.session);
    if let Value::Object(object) = &mut value {
        object.insert(
            "messages".to_string(),
            Value::Array(
                exported
                    .messages
                    .into_iter()
                    .map(stored_message_to_json)
                    .collect(),
            ),
        );
    }
    value
}

fn conversation_message_to_json(message: ConversationMessage) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("role".to_string(), json!(message.role));
    object.insert("content".to_string(), json!(message.content));
    insert_optional(&mut object, "tool_call_id", message.tool_call_id);
    insert_optional(&mut object, "tool_name", message.tool_name);
    insert_optional(&mut object, "tool_calls", message.tool_calls);
    insert_optional(&mut object, "finish_reason", message.finish_reason);
    insert_optional(&mut object, "reasoning", message.reasoning);
    insert_optional(&mut object, "reasoning_content", message.reasoning_content);
    insert_optional(&mut object, "reasoning_details", message.reasoning_details);
    insert_optional(
        &mut object,
        "codex_reasoning_items",
        message.codex_reasoning_items,
    );
    insert_optional(
        &mut object,
        "codex_message_items",
        message.codex_message_items,
    );
    Value::Object(object)
}

fn insert_optional<T>(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<T>)
where
    T: serde::Serialize,
{
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value));
    }
}

fn session_to_json(session: SessionRecord) -> Value {
    json!({
        "id": session.id,
        "source": session.source,
        "user_id": session.user_id,
        "model": session.model,
        "model_config": session.model_config,
        "system_prompt": session.system_prompt,
        "parent_session_id": session.parent_session_id,
        "started_at": session.started_at,
        "ended_at": session.ended_at,
        "end_reason": session.end_reason,
        "message_count": session.message_count,
        "tool_call_count": session.tool_call_count,
        "input_tokens": session.input_tokens,
        "output_tokens": session.output_tokens,
        "cache_read_tokens": session.cache_read_tokens,
        "cache_write_tokens": session.cache_write_tokens,
        "reasoning_tokens": session.reasoning_tokens,
        "billing_provider": session.billing_provider,
        "billing_base_url": session.billing_base_url,
        "billing_mode": session.billing_mode,
        "estimated_cost_usd": session.estimated_cost_usd,
        "actual_cost_usd": session.actual_cost_usd,
        "cost_status": session.cost_status,
        "cost_source": session.cost_source,
        "pricing_version": session.pricing_version,
        "title": session.title,
        "api_call_count": session.api_call_count,
    })
}

fn search_match_to_json(search_match: SearchMatch) -> Value {
    json!({
        "id": search_match.id,
        "session_id": search_match.session_id,
        "role": search_match.role,
        "snippet": search_match.snippet,
        "timestamp": search_match.timestamp,
        "tool_name": search_match.tool_name,
        "source": search_match.source,
        "model": search_match.model,
        "session_started": search_match.session_started,
        "context": search_match
            .context
            .into_iter()
            .map(context_to_json)
            .collect::<Vec<_>>(),
    })
}

fn context_to_json(message: SearchContextMessage) -> Value {
    json!({
        "role": message.role,
        "content": message.content,
    })
}

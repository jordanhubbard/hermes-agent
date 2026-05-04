//! SQLite-backed session/message storage.
//!
//! This module ports the core CRUD behavior from Python `SessionDB` while the
//! Python implementation remains authoritative in production.

use crate::schema::{FTS_SQL, FTS_TRIGRAM_SQL, SCHEMA_SQL, SCHEMA_VERSION};
use crate::search::{contains_cjk, count_cjk, sanitize_fts5_query};
use crate::title::{sanitize_title, TitleError};
use regex::Regex;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

const CONTENT_JSON_PREFIX: &str = "\0json:";
const SESSION_COLUMNS: &str = "id, source, user_id, model, model_config, system_prompt, \
    parent_session_id, started_at, ended_at, end_reason, message_count, tool_call_count, \
    input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, \
    billing_provider, billing_base_url, billing_mode, estimated_cost_usd, actual_cost_usd, \
    cost_status, cost_source, pricing_version, title, api_call_count";

pub type StateResult<T> = Result<T, StateError>;

#[derive(Debug)]
pub enum StateError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Title(TitleError),
    TitleConflict { title: String, session_id: String },
    Time(SystemTimeError),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Sqlite(err) => write!(f, "sqlite error: {err}"),
            StateError::Json(err) => write!(f, "json error: {err}"),
            StateError::Title(err) => write!(f, "{err}"),
            StateError::TitleConflict { title, session_id } => {
                write!(
                    f,
                    "Title '{title}' is already in use by session {session_id}"
                )
            }
            StateError::Time(err) => write!(f, "system time error: {err}"),
        }
    }
}

impl std::error::Error for StateError {}

impl From<rusqlite::Error> for StateError {
    fn from(value: rusqlite::Error) -> Self {
        StateError::Sqlite(value)
    }
}

impl From<serde_json::Error> for StateError {
    fn from(value: serde_json::Error) -> Self {
        StateError::Json(value)
    }
}

impl From<TitleError> for StateError {
    fn from(value: TitleError) -> Self {
        StateError::Title(value)
    }
}

impl From<SystemTimeError> for StateError {
    fn from(value: SystemTimeError) -> Self {
        StateError::Time(value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewSession {
    pub id: String,
    pub source: String,
    pub user_id: Option<String>,
    pub model: Option<String>,
    pub model_config: Option<Value>,
    pub system_prompt: Option<String>,
    pub parent_session_id: Option<String>,
}

impl NewSession {
    pub fn new(id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            ..Self::default()
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn parent_session_id(mut self, parent_session_id: impl Into<String>) -> Self {
        self.parent_session_id = Some(parent_session_id.into());
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppendMessage {
    pub session_id: String,
    pub role: String,
    pub content: Option<Value>,
    pub tool_name: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_call_id: Option<String>,
    pub token_count: Option<i64>,
    pub finish_reason: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning_details: Option<Value>,
    pub codex_reasoning_items: Option<Value>,
    pub codex_message_items: Option<Value>,
}

impl AppendMessage {
    pub fn new(session_id: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            role: role.into(),
            ..Self::default()
        }
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.content = Some(Value::String(content.into()));
        self
    }

    pub fn tool_calls(mut self, tool_calls: Value) -> Self {
        self.tool_calls = Some(tool_calls);
        self
    }

    pub fn tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = Some(tool_name.into());
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct TokenUpdate {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub reasoning_tokens: i64,
    pub estimated_cost_usd: Option<f64>,
    pub actual_cost_usd: Option<f64>,
    pub cost_status: Option<String>,
    pub cost_source: Option<String>,
    pub pricing_version: Option<String>,
    pub billing_provider: Option<String>,
    pub billing_base_url: Option<String>,
    pub billing_mode: Option<String>,
    pub model: Option<String>,
    pub api_call_count: i64,
    pub absolute: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub id: String,
    pub source: String,
    pub user_id: Option<String>,
    pub model: Option<String>,
    pub model_config: Option<String>,
    pub system_prompt: Option<String>,
    pub parent_session_id: Option<String>,
    pub started_at: f64,
    pub ended_at: Option<f64>,
    pub end_reason: Option<String>,
    pub message_count: i64,
    pub tool_call_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub reasoning_tokens: i64,
    pub billing_provider: Option<String>,
    pub billing_base_url: Option<String>,
    pub billing_mode: Option<String>,
    pub estimated_cost_usd: Option<f64>,
    pub actual_cost_usd: Option<f64>,
    pub cost_status: Option<String>,
    pub cost_source: Option<String>,
    pub pricing_version: Option<String>,
    pub title: Option<String>,
    pub api_call_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionListOptions {
    pub source: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for SessionListOptions {
    fn default() -> Self {
        Self {
            source: None,
            limit: 20,
            offset: 0,
        }
    }
}

impl SessionListOptions {
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = offset;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionListRecord {
    pub session: SessionRecord,
    pub last_active: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRichOptions {
    pub source: Option<String>,
    pub exclude_sources: Option<Vec<String>>,
    pub limit: i64,
    pub offset: i64,
    pub include_children: bool,
    pub project_compression_tips: bool,
    pub order_by_last_active: bool,
}

impl Default for SessionRichOptions {
    fn default() -> Self {
        Self {
            source: None,
            exclude_sources: None,
            limit: 20,
            offset: 0,
            include_children: false,
            project_compression_tips: true,
            order_by_last_active: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRichRecord {
    pub session: SessionRecord,
    pub preview: String,
    pub last_active: f64,
    pub lineage_root_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportedSession {
    pub session: SessionRecord,
    pub messages: Vec<StoredMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: Option<Value>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_name: Option<String>,
    pub timestamp: f64,
    pub token_count: Option<i64>,
    pub finish_reason: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning_details: Option<Value>,
    pub codex_reasoning_items: Option<Value>,
    pub codex_message_items: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationMessage {
    pub role: String,
    pub content: Option<Value>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Value>,
    pub tool_name: Option<String>,
    pub finish_reason: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning_details: Option<Value>,
    pub codex_reasoning_items: Option<Value>,
    pub codex_message_items: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchOptions {
    pub source_filter: Option<Vec<String>>,
    pub exclude_sources: Option<Vec<String>>,
    pub role_filter: Option<Vec<String>>,
    pub limit: i64,
    pub offset: i64,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            source_filter: None,
            exclude_sources: None,
            role_filter: None,
            limit: 20,
            offset: 0,
        }
    }
}

impl SearchOptions {
    pub fn source_filter<I, S>(mut self, sources: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.source_filter = Some(sources.into_iter().map(Into::into).collect());
        self
    }

    pub fn exclude_sources<I, S>(mut self, sources: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.exclude_sources = Some(sources.into_iter().map(Into::into).collect());
        self
    }

    pub fn role_filter<I, S>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.role_filter = Some(roles.into_iter().map(Into::into).collect());
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = offset;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchContextMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchMatch {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub snippet: String,
    pub timestamp: f64,
    pub tool_name: Option<String>,
    pub source: String,
    pub model: Option<String>,
    pub session_started: f64,
    pub context: Vec<SearchContextMessage>,
}

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn open(path: impl AsRef<Path>) -> StateResult<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    pub fn open_in_memory() -> StateResult<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> StateResult<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )?;
        self.conn.execute_batch(SCHEMA_SQL)?;
        self.reconcile_columns()?;
        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_title_unique \
             ON sessions(title) WHERE title IS NOT NULL",
            [],
        )?;

        let current: Option<i32> = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .optional()?;

        if matches!(current, Some(version) if version < 11) {
            self.rebuild_fts_indexes()?;
        }
        self.conn.execute_batch(FTS_SQL)?;
        self.conn.execute_batch(FTS_TRIGRAM_SQL)?;

        match current {
            Some(version) if version < SCHEMA_VERSION => {
                self.conn
                    .execute("UPDATE schema_version SET version = ?", [SCHEMA_VERSION])?;
            }
            None => {
                self.conn.execute(
                    "INSERT INTO schema_version (version) VALUES (?)",
                    [SCHEMA_VERSION],
                )?;
            }
            Some(_) => {}
        }
        Ok(())
    }

    fn reconcile_columns(&self) -> StateResult<()> {
        for (table_name, declared_columns) in expected_schema_columns()? {
            let table_name_sql = quote_identifier(&table_name);
            let pragma = format!("PRAGMA table_info(\"{table_name_sql}\")");
            let mut stmt = self.conn.prepare(&pragma)?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let live_columns = rows.collect::<Result<HashSet<_>, _>>()?;

            for (column_name, column_type) in declared_columns {
                if live_columns.contains(&column_name) {
                    continue;
                }

                let column_name_sql = quote_identifier(&column_name);
                let add_column = if column_type.is_empty() {
                    format!("ALTER TABLE \"{table_name_sql}\" ADD COLUMN \"{column_name_sql}\"")
                } else {
                    format!(
                        "ALTER TABLE \"{table_name_sql}\" ADD COLUMN \"{column_name_sql}\" {column_type}"
                    )
                };
                match self.conn.execute(&add_column, []) {
                    Ok(_) => {}
                    Err(err) if sqlite_error_message(&err).contains("duplicate column name") => {}
                    Err(err) => return Err(StateError::Sqlite(err)),
                }
            }
        }
        Ok(())
    }

    fn rebuild_fts_indexes(&self) -> StateResult<()> {
        for trigger in [
            "messages_fts_insert",
            "messages_fts_delete",
            "messages_fts_update",
            "messages_fts_trigram_insert",
            "messages_fts_trigram_delete",
            "messages_fts_trigram_update",
        ] {
            self.conn
                .execute(&format!("DROP TRIGGER IF EXISTS {trigger}"), [])?;
        }
        for table in ["messages_fts", "messages_fts_trigram"] {
            self.conn
                .execute(&format!("DROP TABLE IF EXISTS {table}"), [])?;
        }

        self.conn.execute_batch(FTS_SQL)?;
        self.conn.execute_batch(FTS_TRIGRAM_SQL)?;
        self.conn.execute(
            "INSERT INTO messages_fts(rowid, content)
             SELECT id,
                    COALESCE(content, '') || ' ' ||
                    COALESCE(tool_name, '') || ' ' ||
                    COALESCE(tool_calls, '')
             FROM messages",
            [],
        )?;
        self.conn.execute(
            "INSERT INTO messages_fts_trigram(rowid, content)
             SELECT id,
                    COALESCE(content, '') || ' ' ||
                    COALESCE(tool_name, '') || ' ' ||
                    COALESCE(tool_calls, '')
             FROM messages",
            [],
        )?;
        Ok(())
    }

    pub fn create_session(&mut self, session: NewSession) -> StateResult<String> {
        let id = session.id.clone();
        let model_config = optional_json_string(session.model_config.as_ref())?;
        let started_at = now_ts()?;
        self.conn.execute(
            "INSERT OR IGNORE INTO sessions \
             (id, source, user_id, model, model_config, system_prompt, parent_session_id, started_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                session.id,
                session.source,
                session.user_id,
                session.model,
                model_config,
                session.system_prompt,
                session.parent_session_id,
                started_at
            ],
        )?;
        Ok(id)
    }

    pub fn get_session(&self, session_id: &str) -> StateResult<Option<SessionRecord>> {
        let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = ?");
        self.conn
            .query_row(&sql, [session_id], session_from_row)
            .optional()
            .map_err(StateError::from)
    }

    pub fn end_session(&mut self, session_id: &str, end_reason: &str) -> StateResult<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?, end_reason = ? WHERE id = ? AND ended_at IS NULL",
            params![now_ts()?, end_reason, session_id],
        )?;
        Ok(())
    }

    pub fn reopen_session(&mut self, session_id: &str) -> StateResult<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at = NULL, end_reason = NULL WHERE id = ?",
            [session_id],
        )?;
        Ok(())
    }

    pub fn update_system_prompt(
        &mut self,
        session_id: &str,
        system_prompt: &str,
    ) -> StateResult<()> {
        self.conn.execute(
            "UPDATE sessions SET system_prompt = ? WHERE id = ?",
            params![system_prompt, session_id],
        )?;
        Ok(())
    }

    pub fn set_session_title(
        &mut self,
        session_id: &str,
        title: Option<&str>,
    ) -> StateResult<bool> {
        let title = sanitize_title(title)?;
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<bool> {
            if let Some(title) = title.as_ref() {
                let conflict: Option<String> = self
                    .conn
                    .query_row(
                        "SELECT id FROM sessions WHERE title = ? AND id != ?",
                        params![title, session_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(conflict) = conflict {
                    return Err(StateError::TitleConflict {
                        title: title.clone(),
                        session_id: conflict,
                    });
                }
            }
            let rowcount = self.conn.execute(
                "UPDATE sessions SET title = ? WHERE id = ?",
                params![title, session_id],
            )?;
            Ok(rowcount > 0)
        })();

        match result {
            Ok(updated) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(updated)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn get_session_title(&self, session_id: &str) -> StateResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT title FROM sessions WHERE id = ?",
                params![session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(StateError::from)
            .map(Option::flatten)
    }

    pub fn get_session_by_title(&self, title: &str) -> StateResult<Option<SessionRecord>> {
        let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE title = ?");
        self.conn
            .query_row(&sql, params![title], session_from_row)
            .optional()
            .map_err(StateError::from)
    }

    pub fn resolve_session_by_title(&self, title: &str) -> StateResult<Option<String>> {
        let exact = self.get_session_by_title(title)?;
        let escaped = escape_like_query(title);
        let pattern = format!("{escaped} #%");
        let numbered = self
            .conn
            .query_row(
                "SELECT id FROM sessions
                 WHERE title LIKE ? ESCAPE '\\'
                 ORDER BY started_at DESC LIMIT 1",
                params![pattern],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if numbered.is_some() {
            Ok(numbered)
        } else {
            Ok(exact.map(|session| session.id))
        }
    }

    pub fn get_next_title_in_lineage(&self, base_title: &str) -> StateResult<String> {
        let base = title_lineage_base(base_title);
        let escaped = escape_like_query(&base);
        let pattern = format!("{escaped} #%");
        let mut stmt = self
            .conn
            .prepare("SELECT title FROM sessions WHERE title = ? OR title LIKE ? ESCAPE '\\'")?;
        let rows = stmt.query_map(params![base, pattern], |row| row.get::<_, String>(0))?;
        let existing = rows.collect::<Result<Vec<_>, _>>()?;
        if existing.is_empty() {
            return Ok(base);
        }
        let max_num = existing
            .iter()
            .filter_map(|title| title_lineage_number(title))
            .max()
            .unwrap_or(1);
        Ok(format!("{base} #{}", max_num + 1))
    }

    pub fn update_token_counts(
        &mut self,
        session_id: &str,
        update: TokenUpdate,
    ) -> StateResult<()> {
        if update.absolute {
            self.conn.execute(
                "UPDATE sessions SET
                   input_tokens = ?,
                   output_tokens = ?,
                   cache_read_tokens = ?,
                   cache_write_tokens = ?,
                   reasoning_tokens = ?,
                   estimated_cost_usd = COALESCE(?, 0),
                   actual_cost_usd = CASE WHEN ? IS NULL THEN actual_cost_usd ELSE ? END,
                   cost_status = COALESCE(?, cost_status),
                   cost_source = COALESCE(?, cost_source),
                   pricing_version = COALESCE(?, pricing_version),
                   billing_provider = COALESCE(billing_provider, ?),
                   billing_base_url = COALESCE(billing_base_url, ?),
                   billing_mode = COALESCE(billing_mode, ?),
                   model = COALESCE(model, ?),
                   api_call_count = ?
                 WHERE id = ?",
                params![
                    update.input_tokens,
                    update.output_tokens,
                    update.cache_read_tokens,
                    update.cache_write_tokens,
                    update.reasoning_tokens,
                    update.estimated_cost_usd,
                    update.actual_cost_usd,
                    update.actual_cost_usd,
                    update.cost_status,
                    update.cost_source,
                    update.pricing_version,
                    update.billing_provider,
                    update.billing_base_url,
                    update.billing_mode,
                    update.model,
                    update.api_call_count,
                    session_id
                ],
            )?;
        } else {
            self.conn.execute(
                "UPDATE sessions SET
                   input_tokens = input_tokens + ?,
                   output_tokens = output_tokens + ?,
                   cache_read_tokens = cache_read_tokens + ?,
                   cache_write_tokens = cache_write_tokens + ?,
                   reasoning_tokens = reasoning_tokens + ?,
                   estimated_cost_usd = COALESCE(estimated_cost_usd, 0) + COALESCE(?, 0),
                   actual_cost_usd = CASE
                       WHEN ? IS NULL THEN actual_cost_usd
                       ELSE COALESCE(actual_cost_usd, 0) + ?
                   END,
                   cost_status = COALESCE(?, cost_status),
                   cost_source = COALESCE(?, cost_source),
                   pricing_version = COALESCE(?, pricing_version),
                   billing_provider = COALESCE(billing_provider, ?),
                   billing_base_url = COALESCE(billing_base_url, ?),
                   billing_mode = COALESCE(billing_mode, ?),
                   model = COALESCE(model, ?),
                   api_call_count = COALESCE(api_call_count, 0) + ?
                 WHERE id = ?",
                params![
                    update.input_tokens,
                    update.output_tokens,
                    update.cache_read_tokens,
                    update.cache_write_tokens,
                    update.reasoning_tokens,
                    update.estimated_cost_usd,
                    update.actual_cost_usd,
                    update.actual_cost_usd,
                    update.cost_status,
                    update.cost_source,
                    update.pricing_version,
                    update.billing_provider,
                    update.billing_base_url,
                    update.billing_mode,
                    update.model,
                    update.api_call_count,
                    session_id
                ],
            )?;
        }
        Ok(())
    }

    pub fn append_message(&mut self, message: AppendMessage) -> StateResult<i64> {
        let tool_call_count = tool_call_count(message.tool_calls.as_ref());

        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<i64> {
            let message_id = insert_message_at(&self.conn, &message, now_ts()?)?;
            if tool_call_count > 0 {
                self.conn.execute(
                    "UPDATE sessions SET message_count = message_count + 1, \
                     tool_call_count = tool_call_count + ? WHERE id = ?",
                    params![tool_call_count, message.session_id],
                )?;
            } else {
                self.conn.execute(
                    "UPDATE sessions SET message_count = message_count + 1 WHERE id = ?",
                    [message.session_id],
                )?;
            }
            Ok(message_id)
        })();

        match result {
            Ok(message_id) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(message_id)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn replace_messages(
        &mut self,
        session_id: &str,
        messages: &[AppendMessage],
    ) -> StateResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<()> {
            self.conn.execute(
                "DELETE FROM messages WHERE session_id = ?",
                params![session_id],
            )?;
            self.conn.execute(
                "UPDATE sessions SET message_count = 0, tool_call_count = 0 WHERE id = ?",
                params![session_id],
            )?;

            let mut timestamp = now_ts()?;
            let mut total_tool_calls = 0_i64;
            for message in messages {
                let mut message = message.clone();
                message.session_id = session_id.to_string();
                insert_message_at(&self.conn, &message, timestamp)?;
                total_tool_calls += tool_call_count(message.tool_calls.as_ref());
                timestamp += 0.000_001;
            }

            self.conn.execute(
                "UPDATE sessions SET message_count = ?, tool_call_count = ? WHERE id = ?",
                params![messages.len() as i64, total_tool_calls, session_id],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn get_messages(&self, session_id: &str) -> StateResult<Vec<StoredMessage>> {
        self.get_messages_for_sessions(&[session_id])
    }

    fn get_messages_for_sessions(&self, session_ids: &[&str]) -> StateResult<Vec<StoredMessage>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(session_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, session_id, role, content, tool_call_id, tool_calls, tool_name, \
             timestamp, token_count, finish_reason, reasoning, reasoning_content, \
             reasoning_details, codex_reasoning_items, codex_message_items \
             FROM messages WHERE session_id IN ({placeholders}) ORDER BY timestamp, id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params = rusqlite::params_from_iter(session_ids.iter().copied());
        let rows = stmt.query_map(params, message_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    pub fn get_messages_as_conversation(
        &self,
        session_id: &str,
        include_ancestors: bool,
    ) -> StateResult<Vec<ConversationMessage>> {
        let owned_session_ids = if include_ancestors {
            self.session_lineage_root_to_tip(session_id)?
        } else {
            vec![session_id.to_string()]
        };
        let session_ids = owned_session_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let rows = self.get_messages_for_sessions(&session_ids)?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let mut msg = conversation_from_stored(row);
            if matches!(msg.role.as_str(), "user" | "assistant") {
                if let Some(Value::String(text)) = msg.content.as_ref() {
                    msg.content = Some(Value::String(sanitize_context(text).trim().to_string()));
                }
            }
            if include_ancestors && is_duplicate_replayed_user_message(&messages, &msg) {
                continue;
            }
            messages.push(msg);
        }
        Ok(messages)
    }

    pub fn search_messages(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> StateResult<Vec<SearchMatch>> {
        if query.trim().is_empty() || options.limit == 0 {
            return Ok(Vec::new());
        }

        let query = sanitize_fts5_query(query);
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let mut matches = if contains_cjk(&query) {
            let raw_query = query.trim_matches('"').trim();
            if count_cjk(raw_query) >= 3 {
                let trigram_query = build_trigram_query(raw_query);
                let mut cjk_matches = self.run_fts_search(
                    "messages_fts_trigram",
                    "messages_fts_trigram",
                    &trigram_query,
                    &options,
                )?;
                cjk_matches.extend(self.run_like_search(raw_query, &options)?);
                dedup_search_matches(cjk_matches)
            } else {
                self.run_like_search(raw_query, &options)?
            }
        } else {
            self.run_fts_search("messages_fts", "messages_fts", &query, &options)?
        };

        for search_match in &mut matches {
            search_match.context = self
                .context_for_search_match(search_match.id)
                .unwrap_or_default();
        }
        Ok(matches)
    }

    pub fn search_sessions(
        &self,
        options: SessionListOptions,
    ) -> StateResult<Vec<SessionListRecord>> {
        let mut params = Vec::new();
        let source_filter = if let Some(source) = options.source.as_ref() {
            params.push(SqlValue::Text(source.clone()));
            "WHERE s.source = ? "
        } else {
            ""
        };
        params.push(SqlValue::Integer(options.limit.max(0)));
        params.push(SqlValue::Integer(options.offset.max(0)));

        let sql = format!(
            "SELECT s.id, s.source, s.user_id, s.model, s.model_config, s.system_prompt,
                    s.parent_session_id, s.started_at, s.ended_at, s.end_reason,
                    s.message_count, s.tool_call_count, s.input_tokens, s.output_tokens,
                    s.cache_read_tokens, s.cache_write_tokens, s.reasoning_tokens,
                    s.billing_provider, s.billing_base_url, s.billing_mode,
                    s.estimated_cost_usd, s.actual_cost_usd, s.cost_status,
                    s.cost_source, s.pricing_version, s.title, s.api_call_count,
                    COALESCE(m.last_active, s.started_at) AS last_active
             FROM sessions s
             LEFT JOIN (
                 SELECT session_id, MAX(timestamp) AS last_active
                 FROM messages GROUP BY session_id
             ) m ON m.session_id = s.id
             {source_filter}
             ORDER BY last_active DESC, s.started_at DESC, s.id DESC
             LIMIT ? OFFSET ?"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), session_list_from_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    pub fn list_sessions_rich(
        &self,
        options: SessionRichOptions,
    ) -> StateResult<Vec<SessionRichRecord>> {
        let mut where_clauses = Vec::new();
        let mut where_params = Vec::new();

        if !options.include_children {
            where_clauses.push(
                "(s.parent_session_id IS NULL
                  OR EXISTS (
                      SELECT 1 FROM sessions p
                      WHERE p.id = s.parent_session_id
                        AND p.end_reason = 'branched'
                        AND s.started_at >= p.ended_at
                  ))"
                .to_string(),
            );
        }
        append_rich_filters(&mut where_clauses, &mut where_params, &options);

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };
        let columns = qualified_session_columns("s");
        let sql = if options.order_by_last_active {
            format!(
                "WITH RECURSIVE chain(root_id, cur_id) AS (
                     SELECT s.id, s.id FROM sessions s {where_sql}
                     UNION ALL
                     SELECT c.root_id, child.id
                     FROM chain c
                     JOIN sessions parent ON parent.id = c.cur_id
                     JOIN sessions child ON child.parent_session_id = c.cur_id
                     WHERE parent.end_reason = 'compression'
                       AND child.started_at >= parent.ended_at
                 ),
                 chain_max AS (
                     SELECT
                         root_id,
                         MAX(COALESCE(
                             (SELECT MAX(m.timestamp) FROM messages m WHERE m.session_id = cur_id),
                             (SELECT started_at FROM sessions ss WHERE ss.id = cur_id)
                         )) AS effective_last_active
                     FROM chain
                     GROUP BY root_id
                 )
                 SELECT {columns},
                     COALESCE(
                         (SELECT SUBSTR(REPLACE(REPLACE(m.content, X'0A', ' '), X'0D', ' '), 1, 63)
                          FROM messages m
                          WHERE m.session_id = s.id AND m.role = 'user' AND m.content IS NOT NULL
                          ORDER BY m.timestamp, m.id LIMIT 1),
                         ''
                     ) AS _preview_raw,
                     COALESCE(
                         (SELECT MAX(m2.timestamp) FROM messages m2 WHERE m2.session_id = s.id),
                         s.started_at
                     ) AS last_active,
                     COALESCE(cm.effective_last_active, s.started_at) AS _effective_last_active
                 FROM sessions s
                 LEFT JOIN chain_max cm ON cm.root_id = s.id
                 {where_sql}
                 ORDER BY _effective_last_active DESC, s.started_at DESC, s.id DESC
                 LIMIT ? OFFSET ?"
            )
        } else {
            format!(
                "SELECT {columns},
                    COALESCE(
                        (SELECT SUBSTR(REPLACE(REPLACE(m.content, X'0A', ' '), X'0D', ' '), 1, 63)
                         FROM messages m
                         WHERE m.session_id = s.id AND m.role = 'user' AND m.content IS NOT NULL
                         ORDER BY m.timestamp, m.id LIMIT 1),
                        ''
                    ) AS _preview_raw,
                    COALESCE(
                        (SELECT MAX(m2.timestamp) FROM messages m2 WHERE m2.session_id = s.id),
                        s.started_at
                    ) AS last_active
                 FROM sessions s
                 {where_sql}
                 ORDER BY s.started_at DESC
                 LIMIT ? OFFSET ?"
            )
        };

        let mut params = if options.order_by_last_active {
            let mut params = where_params.clone();
            params.extend(where_params);
            params
        } else {
            where_params
        };
        params.push(SqlValue::Integer(options.limit));
        params.push(SqlValue::Integer(options.offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), session_rich_from_row)?;
        let rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)?;

        if options.project_compression_tips && !options.include_children {
            self.project_compression_roots(rows)
        } else {
            Ok(rows)
        }
    }

    pub fn get_session_rich_row(&self, session_id: &str) -> StateResult<Option<SessionRichRecord>> {
        let columns = qualified_session_columns("s");
        let sql = format!(
            "SELECT {columns},
                COALESCE(
                    (SELECT SUBSTR(REPLACE(REPLACE(m.content, X'0A', ' '), X'0D', ' '), 1, 63)
                     FROM messages m
                     WHERE m.session_id = s.id AND m.role = 'user' AND m.content IS NOT NULL
                     ORDER BY m.timestamp, m.id LIMIT 1),
                    ''
                ) AS _preview_raw,
                COALESCE(
                    (SELECT MAX(m2.timestamp) FROM messages m2 WHERE m2.session_id = s.id),
                    s.started_at
                ) AS last_active
             FROM sessions s
             WHERE s.id = ?"
        );
        self.conn
            .query_row(&sql, params![session_id], session_rich_from_row)
            .optional()
            .map_err(StateError::from)
    }

    pub fn session_count(&self, source: Option<&str>) -> StateResult<i64> {
        match source {
            Some(source) => self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sessions WHERE source = ?",
                    params![source],
                    |row| row.get(0),
                )
                .map_err(StateError::from),
            None => self
                .conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .map_err(StateError::from),
        }
    }

    pub fn message_count(&self, session_id: Option<&str>) -> StateResult<i64> {
        match session_id {
            Some(session_id) => self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM messages WHERE session_id = ?",
                    params![session_id],
                    |row| row.get(0),
                )
                .map_err(StateError::from),
            None => self
                .conn
                .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
                .map_err(StateError::from),
        }
    }

    pub fn export_session(&self, session_id: &str) -> StateResult<Option<ExportedSession>> {
        let Some(session) = self.get_session(session_id)? else {
            return Ok(None);
        };
        let messages = self.get_messages(session_id)?;
        Ok(Some(ExportedSession { session, messages }))
    }

    pub fn export_all(&self, source: Option<&str>) -> StateResult<Vec<ExportedSession>> {
        let sessions = self.search_sessions(SessionListOptions {
            source: source.map(str::to_string),
            limit: 100_000,
            offset: 0,
        })?;
        let mut exports = Vec::with_capacity(sessions.len());
        for row in sessions {
            let messages = self.get_messages(&row.session.id)?;
            exports.push(ExportedSession {
                session: row.session,
                messages,
            });
        }
        Ok(exports)
    }

    pub fn clear_messages(&mut self, session_id: &str) -> StateResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<()> {
            self.conn.execute(
                "DELETE FROM messages WHERE session_id = ?",
                params![session_id],
            )?;
            self.conn.execute(
                "UPDATE sessions SET message_count = 0, tool_call_count = 0 WHERE id = ?",
                params![session_id],
            )?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn delete_session(&mut self, session_id: &str) -> StateResult<bool> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<bool> {
            let exists: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE id = ?",
                params![session_id],
                |row| row.get(0),
            )?;
            if exists == 0 {
                return Ok(false);
            }
            self.conn.execute(
                "UPDATE sessions SET parent_session_id = NULL WHERE parent_session_id = ?",
                params![session_id],
            )?;
            self.conn.execute(
                "DELETE FROM messages WHERE session_id = ?",
                params![session_id],
            )?;
            self.conn
                .execute("DELETE FROM sessions WHERE id = ?", params![session_id])?;
            Ok(true)
        })();

        match result {
            Ok(deleted) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(deleted)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn resolve_session_id(&self, session_id_or_prefix: &str) -> StateResult<Option<String>> {
        if let Some(session) = self.get_session(session_id_or_prefix)? {
            return Ok(Some(session.id));
        }

        let escaped = escape_like_query(session_id_or_prefix);
        let pattern = format!("{escaped}%");
        let mut stmt = self.conn.prepare(
            "SELECT id FROM sessions
             WHERE id LIKE ? ESCAPE '\\'
             ORDER BY started_at DESC LIMIT 2",
        )?;
        let rows = stmt.query_map(params![pattern], |row| row.get::<_, String>(0))?;
        let matches = rows.collect::<Result<Vec<_>, _>>()?;
        if matches.len() == 1 {
            Ok(matches.into_iter().next())
        } else {
            Ok(None)
        }
    }

    pub fn resolve_resume_session_id(&self, session_id: &str) -> StateResult<String> {
        if session_id.is_empty() {
            return Ok(session_id.to_string());
        }

        let has_messages: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM messages WHERE session_id = ? LIMIT 1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        if has_messages.is_some() {
            return Ok(session_id.to_string());
        }

        let mut current = session_id.to_string();
        let mut seen = HashSet::from([current.clone()]);
        for _ in 0..32 {
            let child_id: Option<String> = self
                .conn
                .query_row(
                    "SELECT id FROM sessions
                     WHERE parent_session_id = ?
                     ORDER BY started_at DESC, id DESC LIMIT 1",
                    params![current],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(child_id) = child_id else {
                return Ok(session_id.to_string());
            };
            if child_id.is_empty() || !seen.insert(child_id.clone()) {
                return Ok(session_id.to_string());
            }
            let child_has_messages: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM messages WHERE session_id = ? LIMIT 1",
                    params![child_id],
                    |row| row.get(0),
                )
                .optional()?;
            if child_has_messages.is_some() {
                return Ok(child_id);
            }
            current = child_id;
        }
        Ok(session_id.to_string())
    }

    pub fn get_compression_tip(&self, session_id: &str) -> StateResult<String> {
        let mut current = session_id.to_string();
        for _ in 0..100 {
            let child_id: Option<String> = self
                .conn
                .query_row(
                    "SELECT id FROM sessions
                     WHERE parent_session_id = ?
                       AND started_at >= (
                           SELECT ended_at FROM sessions
                           WHERE id = ? AND end_reason = 'compression'
                       )
                     ORDER BY started_at DESC LIMIT 1",
                    params![current, current],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(child_id) = child_id else {
                return Ok(current);
            };
            current = child_id;
        }
        Ok(current)
    }

    pub fn prune_empty_ghost_sessions(&mut self) -> StateResult<Vec<String>> {
        let cutoff = now_ts()? - 86_400.0;
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<Vec<String>> {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM sessions
                 WHERE source = 'tui'
                   AND title IS NULL
                   AND ended_at IS NOT NULL
                   AND started_at < ?
                   AND NOT EXISTS (
                       SELECT 1 FROM messages WHERE messages.session_id = sessions.id
                   )",
            )?;
            let ids = stmt
                .query_map(params![cutoff], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            if !ids.is_empty() {
                let placeholders = placeholders(ids.len());
                self.conn.execute(
                    &format!("DELETE FROM sessions WHERE id IN ({placeholders})"),
                    params_from_iter(ids.iter()),
                )?;
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(ids)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn prune_sessions(
        &mut self,
        older_than_days: i64,
        source: Option<&str>,
    ) -> StateResult<Vec<String>> {
        let cutoff = now_ts()? - (older_than_days as f64 * 86_400.0);
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> StateResult<Vec<String>> {
            let ids = if let Some(source) = source {
                let mut stmt = self.conn.prepare(
                    "SELECT id FROM sessions
                     WHERE started_at < ? AND ended_at IS NOT NULL AND source = ?",
                )?;
                let rows =
                    stmt.query_map(params![cutoff, source], |row| row.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            } else {
                let mut stmt = self.conn.prepare(
                    "SELECT id FROM sessions WHERE started_at < ? AND ended_at IS NOT NULL",
                )?;
                let rows = stmt.query_map(params![cutoff], |row| row.get::<_, String>(0))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            if ids.is_empty() {
                return Ok(ids);
            }

            let placeholders = placeholders(ids.len());
            self.conn.execute(
                &format!(
                    "UPDATE sessions SET parent_session_id = NULL
                     WHERE parent_session_id IN ({placeholders})"
                ),
                params_from_iter(ids.iter()),
            )?;
            for id in &ids {
                self.conn
                    .execute("DELETE FROM messages WHERE session_id = ?", params![id])?;
                self.conn
                    .execute("DELETE FROM sessions WHERE id = ?", params![id])?;
            }
            Ok(ids)
        })();
        match result {
            Ok(ids) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(ids)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn get_meta(&self, key: &str) -> StateResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM state_meta WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(StateError::from)
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> StateResult<()> {
        self.conn.execute(
            "INSERT INTO state_meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn vacuum(&self) -> StateResult<()> {
        let _ = self.conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", []);
        self.conn.execute("VACUUM", [])?;
        Ok(())
    }

    pub fn session_lineage_root_to_tip(&self, session_id: &str) -> StateResult<Vec<String>> {
        if session_id.is_empty() {
            return Ok(vec![session_id.to_string()]);
        }

        let mut chain = Vec::new();
        let mut current = Some(session_id.to_string());
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            let Some(id) = current else {
                break;
            };
            if !seen.insert(id.clone()) {
                break;
            }
            chain.push(id.clone());
            current = self
                .conn
                .query_row(
                    "SELECT parent_session_id FROM sessions WHERE id = ?",
                    params![id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
        }
        if chain.is_empty() {
            Ok(vec![session_id.to_string()])
        } else {
            chain.reverse();
            Ok(chain)
        }
    }

    pub fn schema_version(&self) -> StateResult<i32> {
        self.conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .map_err(StateError::from)
    }

    fn run_fts_search(
        &self,
        table_name: &str,
        snippet_table_name: &str,
        query: &str,
        options: &SearchOptions,
    ) -> StateResult<Vec<SearchMatch>> {
        let mut where_clauses = vec![format!("{table_name} MATCH ?")];
        let mut params = vec![SqlValue::Text(query.to_string())];
        if !append_search_filters(&mut where_clauses, &mut params, options) {
            return Ok(Vec::new());
        }
        append_limit_offset(&mut params, options);

        let sql = format!(
            "SELECT
                m.id,
                m.session_id,
                m.role,
                snippet({snippet_table_name}, 0, '>>>', '<<<', '...', 40) AS snippet,
                m.timestamp,
                m.tool_name,
                s.source,
                s.model,
                s.started_at AS session_started
             FROM {table_name}
             JOIN messages m ON m.id = {table_name}.rowid
             JOIN sessions s ON s.id = m.session_id
             WHERE {}
             ORDER BY rank
             LIMIT ? OFFSET ?",
            where_clauses.join(" AND ")
        );
        self.run_search_sql(&sql, params, true)
    }

    fn run_like_search(
        &self,
        raw_query: &str,
        options: &SearchOptions,
    ) -> StateResult<Vec<SearchMatch>> {
        let escaped = escape_like_query(raw_query);
        let pattern = format!("%{escaped}%");
        let mut where_clauses = vec![
            "(m.content LIKE ? ESCAPE '\\' OR m.tool_name LIKE ? ESCAPE '\\' OR m.tool_calls LIKE ? ESCAPE '\\')"
                .to_string(),
        ];
        let mut params = vec![
            SqlValue::Text(pattern.clone()),
            SqlValue::Text(pattern.clone()),
            SqlValue::Text(pattern),
        ];
        if !append_search_filters(&mut where_clauses, &mut params, options) {
            return Ok(Vec::new());
        }

        let mut sql_params = vec![SqlValue::Text(raw_query.to_string())];
        sql_params.extend(params);
        append_limit_offset(&mut sql_params, options);

        let sql = format!(
            "SELECT
                m.id,
                m.session_id,
                m.role,
                substr(m.content, max(1, instr(m.content, ?) - 40), 120) AS snippet,
                m.timestamp,
                m.tool_name,
                s.source,
                s.model,
                s.started_at AS session_started
             FROM messages m
             JOIN sessions s ON s.id = m.session_id
             WHERE {}
             ORDER BY m.timestamp DESC
             LIMIT ? OFFSET ?",
            where_clauses.join(" AND ")
        );
        self.run_search_sql(&sql, sql_params, false)
    }

    fn run_search_sql(
        &self,
        sql: &str,
        params: Vec<SqlValue>,
        swallow_sql_errors: bool,
    ) -> StateResult<Vec<SearchMatch>> {
        let mut stmt = match self.conn.prepare(sql) {
            Ok(stmt) => stmt,
            Err(err) if swallow_sql_errors => {
                let _ = err;
                return Ok(Vec::new());
            }
            Err(err) => return Err(StateError::Sqlite(err)),
        };
        let rows = match stmt.query_map(params_from_iter(params.iter()), search_match_from_row) {
            Ok(rows) => rows,
            Err(err) if swallow_sql_errors => {
                let _ = err;
                return Ok(Vec::new());
            }
            Err(err) => return Err(StateError::Sqlite(err)),
        };
        let mut matches = Vec::new();
        for row in rows {
            match row {
                Ok(search_match) => matches.push(search_match),
                Err(err) if swallow_sql_errors => {
                    let _ = err;
                    return Ok(Vec::new());
                }
                Err(err) => return Err(StateError::Sqlite(err)),
            }
        }
        Ok(matches)
    }

    fn context_for_search_match(&self, message_id: i64) -> StateResult<Vec<SearchContextMessage>> {
        let mut stmt = self.conn.prepare(
            "WITH target AS (
                 SELECT session_id, timestamp, id
                 FROM messages
                 WHERE id = ?
             )
             SELECT role, content
             FROM (
                 SELECT m.id, m.timestamp, m.role, m.content
                 FROM messages m
                 JOIN target t ON t.session_id = m.session_id
                 WHERE (m.timestamp < t.timestamp)
                    OR (m.timestamp = t.timestamp AND m.id < t.id)
                 ORDER BY m.timestamp DESC, m.id DESC
                 LIMIT 1
             )
             UNION ALL
             SELECT role, content
             FROM messages
             WHERE id = ?
             UNION ALL
             SELECT role, content
             FROM (
                 SELECT m.id, m.timestamp, m.role, m.content
                 FROM messages m
                 JOIN target t ON t.session_id = m.session_id
                 WHERE (m.timestamp > t.timestamp)
                    OR (m.timestamp = t.timestamp AND m.id > t.id)
                 ORDER BY m.timestamp ASC, m.id ASC
                 LIMIT 1
             )",
        )?;
        let rows = stmt.query_map(params![message_id, message_id], |row| {
            let role: String = row.get(0)?;
            let raw_content: Option<String> = row.get(1)?;
            let content =
                preview_search_context(raw_content.as_deref()).map_err(json_to_sql_error)?;
            Ok(SearchContextMessage { role, content })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    fn project_compression_roots(
        &self,
        rows: Vec<SessionRichRecord>,
    ) -> StateResult<Vec<SessionRichRecord>> {
        let mut projected = Vec::with_capacity(rows.len());
        for mut row in rows {
            if row.session.end_reason.as_deref() != Some("compression") {
                projected.push(row);
                continue;
            }

            let root_id = row.session.id.clone();
            let tip_id = self.get_compression_tip(&root_id)?;
            if tip_id == root_id {
                projected.push(row);
                continue;
            }

            let Some(tip_row) = self.get_session_rich_row(&tip_id)? else {
                projected.push(row);
                continue;
            };

            let SessionRichRecord {
                session: tip_session,
                preview,
                last_active,
                ..
            } = tip_row;
            let root_started_at = row.session.started_at;
            row.session.id = tip_session.id;
            row.session.ended_at = tip_session.ended_at;
            row.session.end_reason = tip_session.end_reason;
            row.session.message_count = tip_session.message_count;
            row.session.tool_call_count = tip_session.tool_call_count;
            row.session.title = tip_session.title;
            row.session.model = tip_session.model;
            row.session.system_prompt = tip_session.system_prompt;
            row.session.started_at = root_started_at;
            row.preview = preview;
            row.last_active = last_active;
            row.lineage_root_id = Some(root_id);
            projected.push(row);
        }
        Ok(projected)
    }
}

fn insert_message_at(
    conn: &Connection,
    message: &AppendMessage,
    timestamp: f64,
) -> StateResult<i64> {
    let stored_content = encode_content(message.content.as_ref())?;
    let tool_calls_json = optional_truthy_json_string(message.tool_calls.as_ref())?;
    let reasoning_details_json = optional_truthy_json_string(message.reasoning_details.as_ref())?;
    let codex_reasoning_items_json =
        optional_truthy_json_string(message.codex_reasoning_items.as_ref())?;
    let codex_message_items_json =
        optional_truthy_json_string(message.codex_message_items.as_ref())?;

    conn.execute(
        "INSERT INTO messages \
         (session_id, role, content, tool_call_id, tool_calls, tool_name, timestamp, \
          token_count, finish_reason, reasoning, reasoning_content, reasoning_details, \
          codex_reasoning_items, codex_message_items) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            &message.session_id,
            &message.role,
            stored_content,
            &message.tool_call_id,
            tool_calls_json,
            &message.tool_name,
            timestamp,
            &message.token_count,
            &message.finish_reason,
            &message.reasoning,
            &message.reasoning_content,
            reasoning_details_json,
            codex_reasoning_items_json,
            codex_message_items_json
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn search_match_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchMatch> {
    Ok(SearchMatch {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        snippet: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        timestamp: row.get(4)?,
        tool_name: row.get(5)?,
        source: row.get(6)?,
        model: row.get(7)?,
        session_started: row.get(8)?,
        context: Vec::new(),
    })
}

fn append_search_filters(
    where_clauses: &mut Vec<String>,
    params: &mut Vec<SqlValue>,
    options: &SearchOptions,
) -> bool {
    if let Some(sources) = options.source_filter.as_ref() {
        if sources.is_empty() {
            return false;
        }
        where_clauses.push(format!("s.source IN ({})", placeholders(sources.len())));
        params.extend(sources.iter().cloned().map(SqlValue::Text));
    }

    if let Some(sources) = options.exclude_sources.as_ref() {
        if !sources.is_empty() {
            where_clauses.push(format!("s.source NOT IN ({})", placeholders(sources.len())));
            params.extend(sources.iter().cloned().map(SqlValue::Text));
        }
    }

    if let Some(roles) = options.role_filter.as_ref() {
        if !roles.is_empty() {
            where_clauses.push(format!("m.role IN ({})", placeholders(roles.len())));
            params.extend(roles.iter().cloned().map(SqlValue::Text));
        }
    }

    true
}

fn append_limit_offset(params: &mut Vec<SqlValue>, options: &SearchOptions) {
    params.push(SqlValue::Integer(options.limit.max(0)));
    params.push(SqlValue::Integer(options.offset.max(0)));
}

fn placeholders(count: usize) -> String {
    std::iter::repeat("?")
        .take(count)
        .collect::<Vec<_>>()
        .join(",")
}

fn expected_schema_columns() -> StateResult<HashMap<String, Vec<(String, String)>>> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA_SQL)?;

    let mut table_stmt = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let tables = table_stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut columns_by_table = HashMap::new();
    for table in tables {
        let table_sql = quote_identifier(&table);
        let pragma = format!("PRAGMA table_info(\"{table_sql}\")");
        let mut column_stmt = conn.prepare(&pragma)?;
        let columns = column_stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let column_type: String = row.get(2)?;
                let not_null: i64 = row.get(3)?;
                let default_value: Option<String> = row.get(4)?;
                let primary_key: i64 = row.get(5)?;

                let mut parts = Vec::new();
                if !column_type.is_empty() {
                    parts.push(column_type);
                }
                if not_null != 0 && primary_key == 0 {
                    parts.push("NOT NULL".to_string());
                }
                if let Some(default_value) = default_value {
                    parts.push(format!("DEFAULT {default_value}"));
                }
                Ok((name, parts.join(" ")))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        columns_by_table.insert(table, columns);
    }

    Ok(columns_by_table)
}

fn quote_identifier(identifier: &str) -> String {
    identifier.replace('"', "\"\"")
}

fn sqlite_error_message(err: &rusqlite::Error) -> String {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(message)) => message.clone(),
        _ => err.to_string(),
    }
}

fn build_trigram_query(raw_query: &str) -> String {
    raw_query
        .split_whitespace()
        .map(|token| {
            if matches!(token.to_ascii_uppercase().as_str(), "AND" | "OR" | "NOT") {
                token.to_string()
            } else {
                format!("\"{}\"", token.replace('"', "\"\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_like_query(raw_query: &str) -> String {
    raw_query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn title_lineage_base(title: &str) -> String {
    let Some((base, suffix)) = title.rsplit_once(" #") else {
        return title.to_string();
    };
    if !base.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) {
        base.to_string()
    } else {
        title.to_string()
    }
}

fn title_lineage_number(title: &str) -> Option<i64> {
    let (_base, suffix) = title.rsplit_once(" #")?;
    suffix.parse().ok()
}

fn dedup_search_matches(matches: Vec<SearchMatch>) -> Vec<SearchMatch> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(matches.len());
    for search_match in matches {
        if seen.insert(search_match.id) {
            deduped.push(search_match);
        }
    }
    deduped
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: row.get(0)?,
        source: row.get(1)?,
        user_id: row.get(2)?,
        model: row.get(3)?,
        model_config: row.get(4)?,
        system_prompt: row.get(5)?,
        parent_session_id: row.get(6)?,
        started_at: row.get(7)?,
        ended_at: row.get(8)?,
        end_reason: row.get(9)?,
        message_count: row.get(10)?,
        tool_call_count: row.get(11)?,
        input_tokens: row.get(12)?,
        output_tokens: row.get(13)?,
        cache_read_tokens: row.get(14)?,
        cache_write_tokens: row.get(15)?,
        reasoning_tokens: row.get(16)?,
        billing_provider: row.get(17)?,
        billing_base_url: row.get(18)?,
        billing_mode: row.get(19)?,
        estimated_cost_usd: row.get(20)?,
        actual_cost_usd: row.get(21)?,
        cost_status: row.get(22)?,
        cost_source: row.get(23)?,
        pricing_version: row.get(24)?,
        title: row.get(25)?,
        api_call_count: row.get(26)?,
    })
}

fn session_list_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionListRecord> {
    Ok(SessionListRecord {
        session: session_from_row(row)?,
        last_active: row.get(27)?,
    })
}

fn session_rich_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRichRecord> {
    let raw_preview: String = row.get(27)?;
    Ok(SessionRichRecord {
        session: session_from_row(row)?,
        preview: rich_preview_from_raw(&raw_preview),
        last_active: row.get(28)?,
        lineage_root_id: None,
    })
}

fn append_rich_filters(
    where_clauses: &mut Vec<String>,
    params: &mut Vec<SqlValue>,
    options: &SessionRichOptions,
) {
    if let Some(source) = options.source.as_ref().filter(|source| !source.is_empty()) {
        where_clauses.push("s.source = ?".to_string());
        params.push(SqlValue::Text(source.clone()));
    }

    if let Some(sources) = options.exclude_sources.as_ref() {
        if !sources.is_empty() {
            where_clauses.push(format!("s.source NOT IN ({})", placeholders(sources.len())));
            params.extend(sources.iter().cloned().map(SqlValue::Text));
        }
    }
}

fn qualified_session_columns(alias: &str) -> String {
    SESSION_COLUMNS
        .split(',')
        .map(str::trim)
        .map(|column| format!("{alias}.{column}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn rich_preview_from_raw(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let mut chars = raw.chars();
    let preview = chars.by_ref().take(60).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

fn conversation_from_stored(row: StoredMessage) -> ConversationMessage {
    let is_assistant = row.role == "assistant";
    ConversationMessage {
        role: row.role,
        content: row.content,
        tool_call_id: row.tool_call_id,
        tool_calls: row.tool_calls,
        tool_name: row.tool_name,
        finish_reason: is_assistant.then_some(row.finish_reason).flatten(),
        reasoning: is_assistant
            .then_some(row.reasoning.filter(|value| !value.is_empty()))
            .flatten(),
        reasoning_content: is_assistant.then_some(row.reasoning_content).flatten(),
        reasoning_details: is_assistant.then_some(row.reasoning_details).flatten(),
        codex_reasoning_items: is_assistant.then_some(row.codex_reasoning_items).flatten(),
        codex_message_items: is_assistant.then_some(row.codex_message_items).flatten(),
    }
}

fn is_duplicate_replayed_user_message(
    messages: &[ConversationMessage],
    msg: &ConversationMessage,
) -> bool {
    if msg.role != "user" {
        return false;
    }
    let Some(Value::String(content)) = msg.content.as_ref() else {
        return false;
    };
    if content.is_empty() {
        return false;
    }

    for prev in messages.iter().rev() {
        if prev.role == "user" && prev.content.as_ref() == Some(&Value::String(content.clone())) {
            return true;
        }
        if prev.role == "assistant" && (prev.content.is_some() || prev.tool_calls.is_some()) {
            return false;
        }
    }
    false
}

fn message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
    let raw_content: Option<String> = row.get(3)?;
    let raw_tool_calls: Option<String> = row.get(5)?;
    let raw_reasoning_details: Option<String> = row.get(12)?;
    let raw_codex_reasoning_items: Option<String> = row.get(13)?;
    let raw_codex_message_items: Option<String> = row.get(14)?;

    Ok(StoredMessage {
        id: row.get(0)?,
        session_id: row.get(1)?,
        role: row.get(2)?,
        content: decode_content(raw_content.as_deref()).map_err(json_to_sql_error)?,
        tool_call_id: row.get(4)?,
        tool_calls: parse_optional_json(raw_tool_calls.as_deref()).map_err(json_to_sql_error)?,
        tool_name: row.get(6)?,
        timestamp: row.get(7)?,
        token_count: row.get(8)?,
        finish_reason: row.get(9)?,
        reasoning: row.get(10)?,
        reasoning_content: row.get(11)?,
        reasoning_details: parse_optional_json(raw_reasoning_details.as_deref())
            .map_err(json_to_sql_error)?,
        codex_reasoning_items: parse_optional_json(raw_codex_reasoning_items.as_deref())
            .map_err(json_to_sql_error)?,
        codex_message_items: parse_optional_json(raw_codex_message_items.as_deref())
            .map_err(json_to_sql_error)?,
    })
}

fn now_ts() -> StateResult<f64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}

fn optional_json_string(value: Option<&Value>) -> StateResult<Option<String>> {
    value
        .map(serde_json::to_string)
        .transpose()
        .map_err(StateError::from)
}

fn optional_truthy_json_string(value: Option<&Value>) -> StateResult<Option<String>> {
    match value {
        Some(value) if json_truthy(value) => Ok(Some(serde_json::to_string(value)?)),
        _ => Ok(None),
    }
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(number) => number.as_f64().map(|v| v != 0.0).unwrap_or(true),
        Value::String(value) => !value.is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
    }
}

fn tool_call_count(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Array(values)) => values.len() as i64,
        Some(_) => 1,
        None => 0,
    }
}

fn encode_content(content: Option<&Value>) -> StateResult<Option<String>> {
    match content {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(value @ (Value::Array(_) | Value::Object(_))) => Ok(Some(format!(
            "{CONTENT_JSON_PREFIX}{}",
            serde_json::to_string(value)?
        ))),
        Some(value) => Ok(Some(value.to_string())),
    }
}

fn decode_content(content: Option<&str>) -> Result<Option<Value>, serde_json::Error> {
    let Some(content) = content else {
        return Ok(None);
    };
    if let Some(encoded) = content.strip_prefix(CONTENT_JSON_PREFIX) {
        serde_json::from_str(encoded).map(Some)
    } else {
        Ok(Some(Value::String(content.to_string())))
    }
}

fn parse_optional_json(value: Option<&str>) -> Result<Option<Value>, serde_json::Error> {
    value.map(serde_json::from_str).transpose()
}

fn preview_search_context(content: Option<&str>) -> Result<String, serde_json::Error> {
    let decoded = decode_content(content)?;
    let preview = match decoded {
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    let object = part.as_object()?;
                    if object.get("type")?.as_str()? == "text" {
                        object.get("text")?.as_str().map(str::to_string)
                    } else {
                        None
                    }
                })
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if text.trim().is_empty() {
                "[multimodal content]".to_string()
            } else {
                text.trim().to_string()
            }
        }
        Some(Value::String(text)) => text,
        _ => String::new(),
    };
    Ok(truncate_chars(&preview, 200))
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}

fn json_to_sql_error(err: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
}

fn memory_context_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?is)<\s*memory-context\s*>.*?</\s*memory-context\s*>")
            .expect("valid memory context regex")
    })
}

fn memory_note_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\[System note:\s*The following is recalled memory context,\s*NOT new user input\.\s*Treat as informational background data\.\]\s*",
        )
        .expect("valid memory note regex")
    })
}

fn memory_fence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)</?\s*memory-context\s*>").expect("valid memory fence regex")
    })
}

fn sanitize_context(text: &str) -> String {
    let text = memory_context_re().replace_all(text, "");
    let text = memory_note_re().replace_all(&text, "");
    memory_fence_re().replace_all(&text, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn initializes_schema() {
        let store = SessionStore::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn creates_and_reads_session() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store
            .create_session(NewSession::new("s1", "cli").model("test-model"))
            .unwrap();

        let session = store.get_session("s1").unwrap().unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.source, "cli");
        assert_eq!(session.model.as_deref(), Some("test-model"));
        assert!(session.ended_at.is_none());
    }

    #[test]
    fn end_session_preserves_first_reason_until_reopen() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store.end_session("s1", "compression").unwrap();
        let first = store.get_session("s1").unwrap().unwrap();

        store.end_session("s1", "user_exit").unwrap();
        let still_first = store.get_session("s1").unwrap().unwrap();
        assert_eq!(still_first.end_reason.as_deref(), Some("compression"));
        assert_eq!(still_first.ended_at, first.ended_at);

        store.reopen_session("s1").unwrap();
        store.end_session("s1", "user_exit").unwrap();
        let reended = store.get_session("s1").unwrap().unwrap();
        assert_eq!(reended.end_reason.as_deref(), Some("user_exit"));
    }

    #[test]
    fn token_updates_increment_and_absolute_set() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .update_token_counts(
                "s1",
                TokenUpdate {
                    input_tokens: 200,
                    output_tokens: 100,
                    api_call_count: 1,
                    model: Some("openai/gpt-5.4".to_string()),
                    ..TokenUpdate::default()
                },
            )
            .unwrap();
        store
            .update_token_counts(
                "s1",
                TokenUpdate {
                    input_tokens: 100,
                    output_tokens: 50,
                    api_call_count: 1,
                    model: Some("anthropic/claude-opus-4.6".to_string()),
                    ..TokenUpdate::default()
                },
            )
            .unwrap();
        let incremented = store.get_session("s1").unwrap().unwrap();
        assert_eq!(incremented.input_tokens, 300);
        assert_eq!(incremented.output_tokens, 150);
        assert_eq!(incremented.api_call_count, 2);
        assert_eq!(incremented.model.as_deref(), Some("openai/gpt-5.4"));

        store
            .update_token_counts(
                "s1",
                TokenUpdate {
                    input_tokens: 10,
                    output_tokens: 20,
                    api_call_count: 7,
                    absolute: true,
                    ..TokenUpdate::default()
                },
            )
            .unwrap();
        let absolute = store.get_session("s1").unwrap().unwrap();
        assert_eq!(absolute.input_tokens, 10);
        assert_eq!(absolute.output_tokens, 20);
        assert_eq!(absolute.api_call_count, 7);
    }

    #[test]
    fn appends_messages_and_counts_tool_calls() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();

        let calls = json!([
            {"id": "call_1", "function": {"name": "ha_call_service", "arguments": "{}"}},
            {"id": "call_2", "function": {"name": "ha_call_service", "arguments": "{}"}}
        ]);
        store
            .append_message(
                AppendMessage::new("s1", "assistant")
                    .text("")
                    .tool_calls(calls.clone()),
            )
            .unwrap();
        store
            .append_message(
                AppendMessage::new("s1", "tool")
                    .text("ok")
                    .tool_name("ha_call_service"),
            )
            .unwrap();

        let session = store.get_session("s1").unwrap().unwrap();
        assert_eq!(session.message_count, 2);
        assert_eq!(session.tool_call_count, 2);

        let messages = store.get_messages("s1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tool_calls.as_ref(), Some(&calls));
        assert_eq!(messages[1].content, Some(Value::String("ok".to_string())));
    }

    #[test]
    fn structured_content_round_trips_with_json_prefix() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        let content = json!([
            {"type": "text", "text": "describe this screenshot"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}}
        ]);

        store
            .append_message(AppendMessage {
                session_id: "s1".to_string(),
                role: "user".to_string(),
                content: Some(content.clone()),
                ..AppendMessage::default()
            })
            .unwrap();

        let messages = store.get_messages("s1").unwrap();
        assert_eq!(messages[0].content.as_ref(), Some(&content));
    }

    #[test]
    fn replace_messages_resets_counts_and_preserves_order() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("old"))
            .unwrap();

        let content = json!([
            {"type": "text", "text": "look at this"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}}
        ]);
        let calls = json!([
            {"id": "c1", "type": "function", "function": {"name": "date", "arguments": "{}"}}
        ]);
        store
            .replace_messages(
                "s1",
                &[
                    AppendMessage {
                        role: "user".to_string(),
                        content: Some(content.clone()),
                        ..AppendMessage::default()
                    },
                    AppendMessage {
                        role: "assistant".to_string(),
                        content: Some(Value::String(String::new())),
                        tool_calls: Some(calls.clone()),
                        ..AppendMessage::default()
                    },
                ],
            )
            .unwrap();

        let session = store.get_session("s1").unwrap().unwrap();
        assert_eq!(session.message_count, 2);
        assert_eq!(session.tool_call_count, 1);

        let messages = store.get_messages("s1").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content.as_ref(), Some(&content));
        assert_eq!(messages[1].tool_calls.as_ref(), Some(&calls));
    }

    #[test]
    fn conversation_replay_restores_assistant_fields_and_strips_memory_context() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        let details = json!([
            {"type": "reasoning.summary", "summary": "Thinking about tools"}
        ]);
        store
            .append_message(AppendMessage {
                session_id: "s1".to_string(),
                role: "assistant".to_string(),
                content: Some(Value::String(
                    "<memory-context>\n[System note: The following is recalled memory context, NOT new user input. Treat as informational background data.]\nstale\n</memory-context>\n\nVisible answer"
                        .to_string(),
                )),
                finish_reason: Some("tool_calls".to_string()),
                reasoning: Some("I should use a tool".to_string()),
                reasoning_content: Some(String::new()),
                reasoning_details: Some(details.clone()),
                ..AppendMessage::default()
            })
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("next"))
            .unwrap();

        let messages = store.get_messages_as_conversation("s1", false).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].content,
            Some(Value::String("Visible answer".to_string()))
        );
        assert_eq!(messages[0].finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(
            messages[0].reasoning.as_deref(),
            Some("I should use a tool")
        );
        assert_eq!(messages[0].reasoning_content.as_deref(), Some(""));
        assert_eq!(messages[0].reasoning_details.as_ref(), Some(&details));
        assert!(messages[1].finish_reason.is_none());
        assert!(messages[1].reasoning.is_none());
    }

    #[test]
    fn conversation_replay_can_include_ancestor_chain_without_duplicate_resume_prompts() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store
            .create_session(NewSession::new("root", "tui"))
            .unwrap();
        store
            .append_message(AppendMessage::new("root", "user").text("same prompt"))
            .unwrap();
        store
            .append_message(AppendMessage::new("root", "user").text("same prompt"))
            .unwrap();
        store
            .append_message(AppendMessage::new("root", "assistant").text("answer"))
            .unwrap();
        store
            .create_session(NewSession::new("child", "tui").parent_session_id("root"))
            .unwrap();
        store
            .append_message(AppendMessage::new("child", "user").text("next prompt"))
            .unwrap();

        let messages = store.get_messages_as_conversation("child", true).unwrap();
        let user_messages = messages
            .iter()
            .filter(|msg| msg.role == "user")
            .map(|msg| msg.content.as_ref().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            user_messages,
            vec![
                &Value::String("same prompt".to_string()),
                &Value::String("next prompt".to_string())
            ]
        );
    }

    #[test]
    fn search_finds_english_content_and_tool_fields() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("How do I deploy with Docker?"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "assistant").text("Use docker compose up."))
            .unwrap();
        store
            .append_message(
                AppendMessage::new("s1", "assistant")
                    .text("")
                    .tool_name("UNIQUETOOLNAME")
                    .tool_calls(json!([{
                        "id": "c1",
                        "type": "function",
                        "function": {
                            "name": "UNIQUEFUNCNAME",
                            "arguments": "{\"query\": \"UNIQUESEARCHTOKEN\"}"
                        }
                    }])),
            )
            .unwrap();

        let docker = store
            .search_messages("docker", SearchOptions::default())
            .unwrap();
        assert_eq!(docker.len(), 2);
        assert!(docker
            .iter()
            .any(|result| result.snippet.to_lowercase().contains("docker")));

        assert_eq!(
            store
                .search_messages("UNIQUETOOLNAME", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .search_messages("UNIQUESEARCHTOKEN", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .search_messages("UNIQUEFUNCNAME", SearchOptions::default())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn search_special_chars_do_not_crash() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("How do I use C++ templates?"))
            .unwrap();

        for query in [
            "C++",
            "\"unterminated",
            "(problem",
            "hello AND",
            "***",
            "{test}",
            "OR hello",
            "a AND OR b",
        ] {
            assert!(store
                .search_messages(query, SearchOptions::default())
                .is_ok());
        }
    }

    #[test]
    fn search_filters_by_source_exclusion_and_role() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("Python from CLI"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "assistant").text("Python answer from CLI"))
            .unwrap();
        store
            .create_session(NewSession::new("s2", "telegram"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "user").text("Python from Telegram"))
            .unwrap();

        let source_filtered = store
            .search_messages(
                "Python",
                SearchOptions::default().source_filter(["telegram"]),
            )
            .unwrap();
        assert_eq!(source_filtered.len(), 1);
        assert_eq!(source_filtered[0].source, "telegram");

        let excluded = store
            .search_messages(
                "Python",
                SearchOptions::default().exclude_sources(["telegram"]),
            )
            .unwrap();
        assert!(excluded.iter().all(|result| result.source != "telegram"));

        let role_filtered = store
            .search_messages(
                "Python",
                SearchOptions::default().role_filter(["assistant"]),
            )
            .unwrap();
        assert_eq!(role_filtered.len(), 1);
        assert_eq!(role_filtered[0].role, "assistant");
    }

    #[test]
    fn search_context_uses_same_session_neighbors() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store.create_session(NewSession::new("s2", "cli")).unwrap();

        store
            .append_message(AppendMessage::new("s1", "user").text("before needle"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "user").text("other session message"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "assistant").text("needle match"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "assistant").text("another session message"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("after needle"))
            .unwrap();

        let results = store
            .search_messages("\"needle match\"", SearchOptions::default())
            .unwrap();
        let needle = results
            .iter()
            .find(|result| result.session_id == "s1")
            .unwrap();

        assert_eq!(
            needle
                .context
                .iter()
                .map(|message| message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["before needle", "needle match", "after needle"]
        );
    }

    #[test]
    fn search_context_renders_multimodal_preview() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .append_message(AppendMessage {
                session_id: "s1".to_string(),
                role: "user".to_string(),
                content: Some(json!([
                    {"type": "text", "text": "describe image"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}}
                ])),
                ..AppendMessage::default()
            })
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "assistant").text("image answer"))
            .unwrap();

        let results = store
            .search_messages("image", SearchOptions::default().role_filter(["assistant"]))
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].context[0].content, "describe image");
    }

    #[test]
    fn search_handles_cjk_trigram_like_and_boolean_queries() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .create_session(NewSession::new("s2", "telegram"))
            .unwrap();
        store
            .append_message(
                AppendMessage::new("s1", "user")
                    .text("昨天和其他Agent的聊天记录，记忆断裂问题复现了"),
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "user").text("今天讨论A2A通信协议的实现"))
            .unwrap();

        let multichar = store
            .search_messages("记忆断裂", SearchOptions::default())
            .unwrap();
        assert_eq!(multichar.len(), 1);
        assert_eq!(multichar[0].session_id, "s1");

        let bigram = store
            .search_messages("通信", SearchOptions::default())
            .unwrap();
        assert_eq!(bigram.len(), 1);
        assert_eq!(bigram[0].session_id, "s2");

        let source_filtered = store
            .search_messages(
                "记忆断裂",
                SearchOptions::default().source_filter(["telegram"]),
            )
            .unwrap();
        assert!(source_filtered.is_empty());

        store.create_session(NewSession::new("s3", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s3", "user").text("断裂连接需要修复"))
            .unwrap();
        let boolean = store
            .search_messages("记忆断裂 OR 断裂连接", SearchOptions::default())
            .unwrap();
        assert_eq!(
            boolean
                .iter()
                .map(|result| result.session_id.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["s1", "s3"])
        );
    }

    #[test]
    fn search_handles_mixed_cjk_english_and_escapes_like_wildcards() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store.create_session(NewSession::new("s2", "cli")).unwrap();
        store.create_session(NewSession::new("s3", "cli")).unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("讨论Agent通信协议"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "user").text("达成100%完成率"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s3", "user").text("达成100完成率是目标"))
            .unwrap();

        let mixed = store
            .search_messages("Agent通信", SearchOptions::default())
            .unwrap();
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].session_id, "s1");

        let wildcard = store
            .search_messages("100%完成", SearchOptions::default())
            .unwrap();
        assert_eq!(wildcard.len(), 1);
        assert_eq!(wildcard[0].session_id, "s2");
    }

    #[test]
    fn lists_sessions_counts_and_exports() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store
            .create_session(NewSession::new("s2", "telegram"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "user").text("first"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s1", "assistant").text("second"))
            .unwrap();
        store
            .append_message(AppendMessage::new("s2", "user").text("third"))
            .unwrap();

        assert_eq!(store.session_count(None).unwrap(), 2);
        assert_eq!(store.session_count(Some("cli")).unwrap(), 1);
        assert_eq!(store.message_count(None).unwrap(), 3);
        assert_eq!(store.message_count(Some("s1")).unwrap(), 2);

        let cli_sessions = store
            .search_sessions(SessionListOptions::default().source("cli"))
            .unwrap();
        assert_eq!(cli_sessions.len(), 1);
        assert_eq!(cli_sessions[0].session.id, "s1");
        assert!(cli_sessions[0].last_active >= cli_sessions[0].session.started_at);

        let all_sessions = store
            .search_sessions(SessionListOptions::default().limit(1).offset(1))
            .unwrap();
        assert_eq!(all_sessions.len(), 1);

        let export = store.export_session("s1").unwrap().unwrap();
        assert_eq!(export.session.source, "cli");
        assert_eq!(export.messages.len(), 2);
        assert!(store.export_session("missing").unwrap().is_none());

        let cli_exports = store.export_all(Some("cli")).unwrap();
        assert_eq!(cli_exports.len(), 1);
        assert_eq!(cli_exports[0].session.id, "s1");
    }

    #[test]
    fn opening_old_schema_reconciles_columns_and_rebuilds_fts() {
        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("state.db");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version (version) VALUES (7);

                 CREATE TABLE sessions (
                     id TEXT PRIMARY KEY,
                     source TEXT NOT NULL,
                     user_id TEXT,
                     model TEXT,
                     model_config TEXT,
                     system_prompt TEXT,
                     parent_session_id TEXT,
                     started_at REAL NOT NULL,
                     ended_at REAL,
                     end_reason TEXT,
                     message_count INTEGER DEFAULT 0,
                     tool_call_count INTEGER DEFAULT 0,
                     input_tokens INTEGER DEFAULT 0,
                     output_tokens INTEGER DEFAULT 0,
                     api_call_count INTEGER DEFAULT 0
                 );

                 CREATE TABLE messages (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id TEXT NOT NULL,
                     role TEXT NOT NULL,
                     content TEXT,
                     tool_call_id TEXT,
                     tool_calls TEXT,
                     tool_name TEXT,
                     timestamp REAL NOT NULL,
                     token_count INTEGER,
                     finish_reason TEXT,
                     reasoning TEXT,
                     reasoning_details TEXT,
                     codex_reasoning_items TEXT
                 );

                 INSERT INTO sessions (id, source, started_at, message_count)
                 VALUES ('existing', 'cli', 1000.0, 1);
                 INSERT INTO messages (session_id, role, content, tool_name, timestamp)
                 VALUES ('existing', 'assistant', '', 'UNIQUETOOLNAME', 1001.0);",
            )
            .unwrap();
        }

        let store = SessionStore::open(&db_path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);

        let session_columns = pragma_columns(&store.conn, "sessions");
        assert!(session_columns.contains("title"));
        assert!(session_columns.contains("cache_read_tokens"));
        assert!(session_columns.contains("reasoning_tokens"));

        let message_columns = pragma_columns(&store.conn, "messages");
        assert!(message_columns.contains("reasoning_content"));
        assert!(message_columns.contains("codex_message_items"));

        let session = store.get_session("existing").unwrap().unwrap();
        assert_eq!(session.title, None);
        assert_eq!(session.cache_read_tokens, 0);
        assert_eq!(session.api_call_count, 0);

        let tool_results = store
            .search_messages("UNIQUETOOLNAME", SearchOptions::default())
            .unwrap();
        assert_eq!(tool_results.len(), 1);
        assert_eq!(tool_results[0].session_id, "existing");
    }

    #[test]
    fn rich_list_builds_previews_and_hides_non_branch_children() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store
            .create_session(NewSession::new("root", "cli"))
            .unwrap();
        store
            .append_message(AppendMessage::new("root", "user").text(format!(
                "{}\n{}",
                "A".repeat(70),
                "tail"
            )))
            .unwrap();
        store
            .create_session(NewSession::new("delegate", "cli").parent_session_id("root"))
            .unwrap();
        store
            .append_message(AppendMessage::new("delegate", "user").text("delegate task"))
            .unwrap();

        store
            .create_session(NewSession::new("branch-parent", "cli"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET ended_at = ?, end_reason = 'branched' WHERE id = ?",
                params![200.0, "branch-parent"],
            )
            .unwrap();
        store
            .create_session(NewSession::new("branch", "cli").parent_session_id("branch-parent"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ? WHERE id = ?",
                params![201.0, "branch"],
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("branch", "user").text("branch task"))
            .unwrap();

        store
            .create_session(NewSession::new("tool-session", "tool"))
            .unwrap();

        let rows = store
            .list_sessions_rich(SessionRichOptions::default())
            .unwrap();
        let ids = rows
            .iter()
            .map(|row| row.session.id.as_str())
            .collect::<HashSet<_>>();
        assert!(ids.contains("root"));
        assert!(ids.contains("branch"));
        assert!(!ids.contains("delegate"));

        let root = rows.iter().find(|row| row.session.id == "root").unwrap();
        assert_eq!(root.preview.len(), 63);
        assert!(root.preview.ends_with("..."));
        assert!(!root.preview.contains('\n'));

        let with_children = store
            .list_sessions_rich(SessionRichOptions {
                include_children: true,
                ..SessionRichOptions::default()
            })
            .unwrap();
        assert!(with_children.iter().any(|row| row.session.id == "delegate"));

        let without_tools = store
            .list_sessions_rich(SessionRichOptions {
                exclude_sources: Some(vec!["tool".to_string()]),
                include_children: true,
                ..SessionRichOptions::default()
            })
            .unwrap();
        assert!(without_tools.iter().all(|row| row.session.source != "tool"));
    }

    #[test]
    fn rich_list_projects_compression_roots_and_orders_by_tip_activity() {
        let mut store = SessionStore::open_in_memory().unwrap();
        let t0 = 1_709_500_000.0;

        store
            .create_session(NewSession::new("root1", "cli").model("root-model"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ?, ended_at = ?, end_reason = 'compression' WHERE id = ?",
                params![t0, t0 + 100.0, "root1"],
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("root1", "user").text("old ask"))
            .unwrap();

        store
            .create_session(NewSession {
                system_prompt: Some("tip system".to_string()),
                ..NewSession::new("tip1", "cli")
                    .model("tip-model")
                    .parent_session_id("root1")
            })
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ? WHERE id = ?",
                params![t0 + 101.0, "tip1"],
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("tip1", "user").text("latest message"))
            .unwrap();
        store.set_session_title("tip1", Some("live tip")).unwrap();
        store
            .conn
            .execute(
                "UPDATE messages SET timestamp = ? WHERE session_id = ? AND content = ?",
                params![t0 + 10_000.0, "tip1", "latest message"],
            )
            .unwrap();

        store
            .create_session(NewSession::new("newer", "cli"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ? WHERE id = ?",
                params![t0 + 500.0, "newer"],
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("newer", "user").text("newer ask"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE messages SET timestamp = ? WHERE session_id = ? AND content = ?",
                params![t0 + 500.0, "newer", "newer ask"],
            )
            .unwrap();

        let top = store
            .list_sessions_rich(SessionRichOptions {
                limit: 1,
                order_by_last_active: true,
                ..SessionRichOptions::default()
            })
            .unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].session.id, "tip1");
        assert_eq!(top[0].lineage_root_id.as_deref(), Some("root1"));
        assert_eq!(top[0].session.started_at, t0);
        assert_eq!(top[0].session.title.as_deref(), Some("live tip"));
        assert_eq!(top[0].session.model.as_deref(), Some("tip-model"));
        assert_eq!(top[0].session.system_prompt.as_deref(), Some("tip system"));
        assert_eq!(top[0].preview, "latest message");

        let raw = store
            .list_sessions_rich(SessionRichOptions {
                project_compression_tips: false,
                ..SessionRichOptions::default()
            })
            .unwrap();
        assert!(raw.iter().any(|row| row.session.id == "root1"));
        assert!(raw.iter().all(|row| row.session.id != "tip1"));
    }

    #[test]
    fn clears_messages_deletes_sessions_and_resolves_prefixes() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store
            .create_session(NewSession::new("20260315_092437_c9a6ff", "cli"))
            .unwrap();
        store
            .create_session(NewSession::new("20260315X092437_c9a6ff", "cli"))
            .unwrap();
        store
            .create_session(NewSession::new("20260315_092437_c9a6aa", "telegram"))
            .unwrap();
        store
            .create_session(NewSession::new("parent", "cli"))
            .unwrap();
        store
            .create_session(NewSession::new("child", "cli").parent_session_id("parent"))
            .unwrap();
        store
            .append_message(AppendMessage::new("parent", "user").text("hello"))
            .unwrap();
        store
            .append_message(AppendMessage::new("child", "user").text("child"))
            .unwrap();

        assert_eq!(
            store
                .resolve_session_id("20260315X092437_c9a6")
                .unwrap()
                .as_deref(),
            Some("20260315X092437_c9a6ff")
        );
        assert!(store
            .resolve_session_id("20260315_092437_c9a6")
            .unwrap()
            .is_none());

        store.clear_messages("parent").unwrap();
        assert_eq!(store.message_count(Some("parent")).unwrap(), 0);
        assert_eq!(
            store.get_session("parent").unwrap().unwrap().message_count,
            0
        );

        assert!(store.delete_session("parent").unwrap());
        assert!(store.get_session("parent").unwrap().is_none());
        assert_eq!(
            store
                .get_session("child")
                .unwrap()
                .unwrap()
                .parent_session_id,
            None
        );
        assert!(!store.delete_session("missing").unwrap());
    }

    #[test]
    fn manages_titles_and_title_lineage() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("s1", "cli")).unwrap();
        store.create_session(NewSession::new("s2", "cli")).unwrap();
        store.create_session(NewSession::new("s3", "cli")).unwrap();

        assert!(store
            .set_session_title("s1", Some("  My\tSession\nTitle  "))
            .unwrap());
        assert_eq!(
            store.get_session_title("s1").unwrap().as_deref(),
            Some("My Session Title")
        );
        assert_eq!(
            store
                .get_session_by_title("My Session Title")
                .unwrap()
                .unwrap()
                .id,
            "s1"
        );
        assert!(!store.set_session_title("missing", Some("Missing")).unwrap());
        assert!(matches!(
            store.set_session_title("s2", Some("My Session Title")),
            Err(StateError::TitleConflict { .. })
        ));

        store
            .set_session_title("s2", Some("My Session Title #2"))
            .unwrap();
        store
            .set_session_title("s3", Some("My Session Title #3"))
            .unwrap();
        assert_eq!(
            store
                .resolve_session_by_title("My Session Title")
                .unwrap()
                .as_deref(),
            Some("s3")
        );
        assert_eq!(
            store.get_next_title_in_lineage("My Session Title").unwrap(),
            "My Session Title #4"
        );
        assert_eq!(
            store
                .get_next_title_in_lineage("My Session Title #2")
                .unwrap(),
            "My Session Title #4"
        );

        assert!(store.set_session_title("s1", Some("   ")).unwrap());
        assert!(store.get_session_title("s1").unwrap().is_none());
    }

    #[test]
    fn resolves_resume_targets_and_compression_tips() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store
            .create_session(NewSession::new("root", "cli"))
            .unwrap();
        store
            .create_session(NewSession::new("empty-child", "cli").parent_session_id("root"))
            .unwrap();
        store
            .create_session(
                NewSession::new("message-child", "cli").parent_session_id("empty-child"),
            )
            .unwrap();
        store
            .append_message(AppendMessage::new("message-child", "user").text("hello"))
            .unwrap();

        assert_eq!(
            store.resolve_resume_session_id("root").unwrap(),
            "message-child"
        );
        assert_eq!(
            store.resolve_resume_session_id("message-child").unwrap(),
            "message-child"
        );
        assert_eq!(
            store.resolve_resume_session_id("missing").unwrap(),
            "missing"
        );
        assert_eq!(store.resolve_resume_session_id("").unwrap(), "");

        store
            .create_session(NewSession::new("croot", "cli"))
            .unwrap();
        store.end_session("croot", "compression").unwrap();
        let parent_end = store
            .get_session("croot")
            .unwrap()
            .unwrap()
            .ended_at
            .unwrap();
        store
            .create_session(NewSession::new("ctip", "cli").parent_session_id("croot"))
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ? WHERE id = ?",
                params![parent_end + 0.001, "ctip"],
            )
            .unwrap();
        assert_eq!(store.get_compression_tip("croot").unwrap(), "ctip");
        assert_eq!(store.get_compression_tip("missing").unwrap(), "missing");
    }

    #[test]
    fn prunes_sessions_and_manages_meta() {
        let mut store = SessionStore::open_in_memory().unwrap();
        store.create_session(NewSession::new("old", "cli")).unwrap();
        store
            .create_session(NewSession::new("active", "cli"))
            .unwrap();
        store
            .create_session(NewSession::new("old-tg", "telegram"))
            .unwrap();
        store
            .create_session(NewSession::new("child", "cli").parent_session_id("old"))
            .unwrap();
        store.end_session("old", "done").unwrap();
        store.end_session("old-tg", "done").unwrap();
        let old_ts = now_ts().unwrap() - 200.0 * 86_400.0;
        for id in ["old", "active", "old-tg"] {
            store
                .conn
                .execute(
                    "UPDATE sessions SET started_at = ? WHERE id = ?",
                    params![old_ts, id],
                )
                .unwrap();
        }

        let pruned = store.prune_sessions(90, Some("cli")).unwrap();
        assert_eq!(pruned, vec!["old".to_string()]);
        assert!(store.get_session("old").unwrap().is_none());
        assert!(store.get_session("active").unwrap().is_some());
        assert_eq!(
            store
                .get_session("child")
                .unwrap()
                .unwrap()
                .parent_session_id,
            None
        );
        assert!(store.get_session("old-tg").unwrap().is_some());

        store.set_meta("last_auto_prune", "123").unwrap();
        assert_eq!(
            store.get_meta("last_auto_prune").unwrap().as_deref(),
            Some("123")
        );
        store.set_meta("last_auto_prune", "456").unwrap();
        assert_eq!(
            store.get_meta("last_auto_prune").unwrap().as_deref(),
            Some("456")
        );

        store
            .create_session(NewSession::new("ghost", "tui"))
            .unwrap();
        store.end_session("ghost", "user_exit").unwrap();
        store
            .conn
            .execute(
                "UPDATE sessions SET started_at = ? WHERE id = ?",
                params![old_ts, "ghost"],
            )
            .unwrap();
        assert_eq!(
            store.prune_empty_ghost_sessions().unwrap(),
            vec!["ghost".to_string()]
        );
        assert!(store.get_session("ghost").unwrap().is_none());
    }

    fn pragma_columns(conn: &Connection, table: &str) -> HashSet<String> {
        let table_sql = quote_identifier(table);
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info(\"{table_sql}\")"))
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<HashSet<_>, _>>()
            .unwrap()
    }
}

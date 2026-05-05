use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MessageEvent {
    pub text: String,
    pub message_type: String,
    pub platform: String,
    pub chat_id: String,
    pub user_id: Option<String>,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub media_urls: Vec<String>,
    pub media_types: Vec<String>,
    pub internal: bool,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SendRequest {
    pub chat_id: String,
    pub content: String,
    pub reply_to: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SendResult {
    pub success: bool,
    pub message_id: Option<String>,
    pub error: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterStatus {
    pub platform: String,
    pub label: String,
    pub connected: bool,
    pub started: bool,
    pub sent_count: usize,
    pub pending_count: usize,
    pub token_lock_scope: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TokenLock {
    pub scope: String,
    pub identity: String,
    pub resource_desc: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlatformEntrySpec {
    pub name: String,
    pub label: String,
    pub required_env: Vec<String>,
    pub install_hint: String,
    pub source: String,
    pub plugin_name: String,
    pub allowed_users_env: String,
    pub allow_all_env: String,
    pub max_message_length: usize,
    pub pii_safe: bool,
    pub emoji: String,
    pub allow_update_command: bool,
    pub platform_hint: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayCommandRoute {
    pub raw: String,
    pub canonical_name: String,
    pub args: String,
    pub known_to_gateway: bool,
    pub bypass_active_session: bool,
    pub action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FreshFinalPlan {
    pub should_send_fresh_final: bool,
    pub calls: Vec<String>,
    pub final_message_id: String,
    pub fallback_to_edit: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetryDecision {
    pub retryable: bool,
    pub timeout: bool,
    pub action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SendRetryPlan {
    pub success: bool,
    pub send_calls: usize,
    pub message_id: Option<String>,
    pub final_error: Option<String>,
    pub fallback_sent: bool,
    pub notice_sent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StreamingDeliverySnapshot {
    pub truncated_plain: Vec<String>,
    pub truncated_code: Vec<String>,
    pub truncated_inline: Vec<String>,
    pub truncated_utf16: Vec<String>,
    pub cleaned_display: String,
    pub thread_metadata: Option<BTreeMap<String, String>>,
    pub runtime_footer: String,
    pub retry_decisions: BTreeMap<String, RetryDecision>,
    pub send_retry_plans: BTreeMap<String, SendRetryPlan>,
    pub fresh_final_success: FreshFinalPlan,
    pub fresh_final_send_failure: FreshFinalPlan,
    pub fresh_final_disabled: FreshFinalPlan,
    pub fresh_final_without_delete: FreshFinalPlan,
    pub fresh_final_short_lived: FreshFinalPlan,
    pub fresh_final_nonfinal: FreshFinalPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl AdapterError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AdapterError {}

#[async_trait]
pub trait GatewayPlatformAdapter {
    fn platform(&self) -> &str;
    fn label(&self) -> &str;
    async fn connect(&mut self) -> Result<AdapterStatus, AdapterError>;
    async fn start(&mut self) -> Result<(), AdapterError>;
    async fn stop(&mut self) -> Result<(), AdapterError>;
    async fn disconnect(&mut self) -> Result<(), AdapterError>;
    async fn send(&mut self, request: SendRequest) -> Result<SendResult, AdapterError>;
    async fn receive(&mut self) -> Result<Option<MessageEvent>, AdapterError>;
    async fn get_chat_info(&self, chat_id: &str) -> Result<BTreeMap<String, String>, AdapterError>;
    fn status(&self) -> AdapterStatus;
    fn acquire_token_lock(&mut self, lock: TokenLock) -> Result<(), AdapterError>;
    fn release_token_lock(&mut self) -> Result<(), AdapterError>;
}

#[derive(Clone, Debug)]
pub struct InMemoryAdapter {
    platform: String,
    label: String,
    connected: bool,
    started: bool,
    sent: Vec<SendRequest>,
    inbound: VecDeque<MessageEvent>,
    token_lock: Option<TokenLock>,
}

impl InMemoryAdapter {
    pub fn new(platform: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            label: label.into(),
            connected: false,
            started: false,
            sent: Vec::new(),
            inbound: VecDeque::new(),
            token_lock: None,
        }
    }

    pub fn queue_event(&mut self, event: MessageEvent) {
        self.inbound.push_back(event);
    }

    pub fn sent_requests(&self) -> &[SendRequest] {
        &self.sent
    }
}

#[async_trait]
impl GatewayPlatformAdapter for InMemoryAdapter {
    fn platform(&self) -> &str {
        &self.platform
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn connect(&mut self) -> Result<AdapterStatus, AdapterError> {
        self.connected = true;
        Ok(self.status())
    }

    async fn start(&mut self) -> Result<(), AdapterError> {
        if !self.connected {
            return Err(AdapterError::new(
                "not_connected",
                "adapter must connect before start",
                false,
            ));
        }
        self.started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), AdapterError> {
        self.started = false;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), AdapterError> {
        self.started = false;
        self.connected = false;
        self.token_lock = None;
        Ok(())
    }

    async fn send(&mut self, request: SendRequest) -> Result<SendResult, AdapterError> {
        if !self.connected {
            return Ok(SendResult {
                success: false,
                message_id: None,
                error: Some("adapter is not connected".to_string()),
                retryable: true,
            });
        }
        let message_id = format!("{}-{}", self.platform, self.sent.len() + 1);
        self.sent.push(request);
        Ok(SendResult {
            success: true,
            message_id: Some(message_id),
            error: None,
            retryable: false,
        })
    }

    async fn receive(&mut self) -> Result<Option<MessageEvent>, AdapterError> {
        Ok(self.inbound.pop_front())
    }

    async fn get_chat_info(&self, chat_id: &str) -> Result<BTreeMap<String, String>, AdapterError> {
        Ok(BTreeMap::from([
            ("id".to_string(), chat_id.to_string()),
            ("name".to_string(), format!("{} {}", self.label, chat_id)),
            ("type".to_string(), "test".to_string()),
        ]))
    }

    fn status(&self) -> AdapterStatus {
        AdapterStatus {
            platform: self.platform.clone(),
            label: self.label.clone(),
            connected: self.connected,
            started: self.started,
            sent_count: self.sent.len(),
            pending_count: self.inbound.len(),
            token_lock_scope: self.token_lock.as_ref().map(|lock| lock.scope.clone()),
        }
    }

    fn acquire_token_lock(&mut self, lock: TokenLock) -> Result<(), AdapterError> {
        if let Some(existing) = &self.token_lock {
            if existing.scope == lock.scope && existing.identity != lock.identity {
                return Err(AdapterError::new(
                    "token_lock_conflict",
                    format!("{} already in use", lock.resource_desc),
                    false,
                ));
            }
        }
        self.token_lock = Some(lock);
        Ok(())
    }

    fn release_token_lock(&mut self) -> Result<(), AdapterError> {
        self.token_lock = None;
        Ok(())
    }
}

pub fn builtin_platform_values() -> &'static [&'static str] {
    &[
        "local",
        "telegram",
        "discord",
        "whatsapp",
        "slack",
        "signal",
        "mattermost",
        "matrix",
        "homeassistant",
        "email",
        "sms",
        "dingtalk",
        "api_server",
        "webhook",
        "feishu",
        "wecom",
        "wecom_callback",
        "weixin",
        "bluebubbles",
        "qqbot",
        "yuanbao",
    ]
}

pub fn adapter_trait_methods() -> &'static [&'static str] {
    &[
        "platform",
        "label",
        "connect",
        "start",
        "stop",
        "disconnect",
        "send",
        "receive",
        "get_chat_info",
        "status",
        "acquire_token_lock",
        "release_token_lock",
    ]
}

pub fn platform_entry_fields() -> &'static [&'static str] {
    &[
        "name",
        "label",
        "required_env",
        "install_hint",
        "source",
        "plugin_name",
        "allowed_users_env",
        "allow_all_env",
        "max_message_length",
        "pii_safe",
        "emoji",
        "allow_update_command",
        "platform_hint",
    ]
}

pub async fn smoke_adapter_roundtrip() -> Result<AdapterStatus, AdapterError> {
    let mut adapter = InMemoryAdapter::new("webhook", "Webhook");
    adapter.acquire_token_lock(TokenLock {
        scope: "webhook_token".to_string(),
        identity: "token-fingerprint".to_string(),
        resource_desc: "Webhook token".to_string(),
    })?;
    adapter.connect().await?;
    adapter.start().await?;
    adapter.queue_event(MessageEvent {
        text: "hello".to_string(),
        message_type: "text".to_string(),
        platform: "webhook".to_string(),
        chat_id: "chat-1".to_string(),
        user_id: Some("user-1".to_string()),
        message_id: Some("msg-1".to_string()),
        thread_id: None,
        media_urls: vec![],
        media_types: vec![],
        internal: false,
        metadata: BTreeMap::new(),
    });
    let event = adapter
        .receive()
        .await?
        .ok_or_else(|| AdapterError::new("missing_event", "inbound event missing", false))?;
    adapter
        .send(SendRequest {
            chat_id: event.chat_id,
            content: format!("echo: {}", event.text),
            reply_to: event.message_id,
            metadata: BTreeMap::new(),
        })
        .await?;
    Ok(adapter.status())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GuardTrace {
    pub depth_after_first_enqueue: usize,
    pub depth_after_second_enqueue: usize,
    pub first_pending_text: Option<String>,
    pub promoted_pending_text: Option<String>,
    pub staged_pending_text: Option<String>,
    pub active_during_drain: bool,
    pub active_after_empty_finish: bool,
    pub stop_bypasses_busy_guard: bool,
    pub status_bypasses_busy_guard: bool,
    pub plain_message_queued: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BusyDecision {
    ProcessCommand,
    QueuePending,
    InterruptRunning,
}

#[derive(Clone, Debug, Default)]
pub struct SessionGuard {
    active_sessions: BTreeSet<String>,
    pending_messages: BTreeMap<String, MessageEvent>,
    queued_events: BTreeMap<String, VecDeque<MessageEvent>>,
}

impl SessionGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_turn(&mut self, session_key: impl Into<String>) {
        self.active_sessions.insert(session_key.into());
    }

    pub fn is_active(&self, session_key: &str) -> bool {
        self.active_sessions.contains(session_key)
    }

    pub fn pending_text(&self, session_key: &str) -> Option<&str> {
        self.pending_messages
            .get(session_key)
            .map(|event| event.text.as_str())
    }

    pub fn enqueue_fifo(&mut self, session_key: impl Into<String>, queued_event: MessageEvent) {
        let key = session_key.into();
        if self.pending_messages.contains_key(&key) {
            self.queued_events
                .entry(key)
                .or_default()
                .push_back(queued_event);
        } else {
            self.pending_messages.insert(key, queued_event);
        }
    }

    pub fn dequeue_pending(&mut self, session_key: &str) -> Option<MessageEvent> {
        self.pending_messages.remove(session_key)
    }

    pub fn promote_queued_event(
        &mut self,
        session_key: &str,
        pending_event: Option<MessageEvent>,
    ) -> Option<MessageEvent> {
        let Some(queue) = self.queued_events.get_mut(session_key) else {
            return pending_event;
        };
        let Some(next_queued) = queue.pop_front() else {
            self.queued_events.remove(session_key);
            return pending_event;
        };
        if queue.is_empty() {
            self.queued_events.remove(session_key);
        }
        if pending_event.is_none() {
            return Some(next_queued);
        }
        self.pending_messages
            .insert(session_key.to_string(), next_queued);
        pending_event
    }

    pub fn queue_depth(&self, session_key: &str) -> usize {
        let mut depth = self
            .queued_events
            .get(session_key)
            .map(VecDeque::len)
            .unwrap_or(0);
        if self.pending_messages.contains_key(session_key) {
            depth += 1;
        }
        depth
    }

    pub fn finish_turn(&mut self, session_key: &str) -> Option<MessageEvent> {
        let pending = self.dequeue_pending(session_key);
        let promoted = self.promote_queued_event(session_key, pending);
        if promoted.is_some() || self.pending_messages.contains_key(session_key) {
            self.active_sessions.insert(session_key.to_string());
        } else {
            self.active_sessions.remove(session_key);
        }
        promoted
    }

    pub fn handle_busy_message(
        &mut self,
        session_key: &str,
        event: MessageEvent,
        busy_mode: &str,
        command_name: Option<&str>,
    ) -> BusyDecision {
        if command_name.is_some_and(should_bypass_active_session) {
            return BusyDecision::ProcessCommand;
        }
        match busy_mode {
            "queue" | "steer" => {
                self.enqueue_fifo(session_key.to_string(), event);
                BusyDecision::QueuePending
            }
            _ => BusyDecision::InterruptRunning,
        }
    }
}

pub fn should_bypass_active_session(command_name: &str) -> bool {
    hermes_cli::resolve_command(command_name).is_some()
}

pub fn route_gateway_command(text: &str) -> Option<GatewayCommandRoute> {
    let dispatch = hermes_cli::parse_slash_dispatch(text)?;
    let canonical = dispatch.canonical_name;
    let args = dispatch.args;
    let known_commands = hermes_cli::gateway_known_commands();
    let known_to_gateway = known_commands.contains(canonical.as_str())
        || hermes_cli::resolve_command(&canonical)
            .map(|cmd| {
                cmd.aliases
                    .iter()
                    .any(|alias| known_commands.contains(*alias))
            })
            .unwrap_or(false);
    if !known_to_gateway {
        return None;
    }
    let action = classify_gateway_action(&canonical, &args);
    Some(GatewayCommandRoute {
        raw: text.trim().to_string(),
        canonical_name: canonical.clone(),
        args,
        known_to_gateway,
        bypass_active_session: should_bypass_active_session(&canonical),
        action,
    })
}

pub fn gateway_command_route_samples() -> BTreeMap<String, Option<GatewayCommandRoute>> {
    [
        "/approve always",
        "/deny",
        "/yolo",
        "/reload-mcp",
        "/reload-skills",
        "/title Project Atlas",
        "/resume sprint-notes",
        "/background run report",
        "/bg run report",
        "/queue next turn",
        "/steer after tool",
        "/status",
        "/help",
        "/verbose",
        "/tools list",
        "/unknown",
    ]
    .into_iter()
    .map(|sample| (sample.to_string(), route_gateway_command(sample)))
    .collect()
}

pub fn streaming_delivery_snapshot() -> StreamingDeliverySnapshot {
    StreamingDeliverySnapshot {
        truncated_plain: truncate_message(
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
            36,
        ),
        truncated_code: truncate_message(
            "Before\n```python\nprint('hello')\nprint('world')\nprint('again')\n```\nAfter",
            44,
        ),
        truncated_inline: truncate_message(
            "Prefix text with `inline code that should not be split (inside)` and suffix text.",
            42,
        ),
        truncated_utf16: truncate_message_utf16("Hello 😀 world 🎵 test 𝄞 ".repeat(8).trim(), 50),
        cleaned_display: clean_for_display(
            "Here is the file:\nMEDIA:/tmp/out.png\n[[audio_as_voice]]\nDone",
        ),
        thread_metadata: thread_metadata("topic-1"),
        runtime_footer: format_runtime_footer(
            Some("openrouter/openai/gpt-5.4"),
            68_000,
            Some(100_000),
            Some("/opt/project"),
            &["model", "context_pct", "cwd"],
            Some("/Users/example"),
        ),
        retry_decisions: retry_decision_samples(),
        send_retry_plans: send_retry_plan_samples(),
        fresh_final_success: plan_fresh_final(60.0, 120.0, true, true, true),
        fresh_final_send_failure: plan_fresh_final(60.0, 120.0, true, true, false),
        fresh_final_disabled: plan_fresh_final(0.0, 120.0, true, true, true),
        fresh_final_without_delete: plan_fresh_final(60.0, 120.0, true, false, true),
        fresh_final_short_lived: plan_fresh_final(60.0, 30.0, true, true, true),
        fresh_final_nonfinal: plan_fresh_final(60.0, 120.0, false, true, true),
    }
}

pub fn thread_metadata(thread_id: &str) -> Option<BTreeMap<String, String>> {
    let trimmed = thread_id.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(BTreeMap::from([(
            "thread_id".to_string(),
            trimmed.to_string(),
        )]))
    }
}

pub fn clean_for_display(text: &str) -> String {
    if !text.contains("MEDIA:") && !text.contains("[[audio_as_voice]]") {
        return text.to_string();
    }
    let without_audio = text.replace("[[audio_as_voice]]", "");
    let media_re = Regex::new(r#"[`"']?MEDIA:\s*\S+[`"']?"#).expect("valid media regex");
    let cleaned = media_re.replace_all(&without_audio, "");
    let blank_re = Regex::new(r"\n{3,}").expect("valid blank-line regex");
    blank_re
        .replace_all(&cleaned, "\n\n")
        .trim_end()
        .to_string()
}

pub fn plan_fresh_final(
    threshold_seconds: f64,
    preview_age_seconds: f64,
    finalize: bool,
    supports_delete: bool,
    fresh_send_success: bool,
) -> FreshFinalPlan {
    let should_send_fresh_final =
        finalize && threshold_seconds > 0.0 && preview_age_seconds >= threshold_seconds;
    if !should_send_fresh_final {
        return FreshFinalPlan {
            should_send_fresh_final: false,
            calls: vec!["send_initial_preview".to_string(), "edit_final".to_string()],
            final_message_id: "initial_preview".to_string(),
            fallback_to_edit: false,
        };
    }
    let mut calls = vec![
        "send_initial_preview".to_string(),
        "send_fresh_final".to_string(),
    ];
    if fresh_send_success {
        if supports_delete {
            calls.push("delete_initial_preview".to_string());
        }
        FreshFinalPlan {
            should_send_fresh_final: true,
            calls,
            final_message_id: "fresh_final".to_string(),
            fallback_to_edit: false,
        }
    } else {
        calls.push("edit_final".to_string());
        FreshFinalPlan {
            should_send_fresh_final: true,
            calls,
            final_message_id: "initial_preview".to_string(),
            fallback_to_edit: true,
        }
    }
}

pub fn truncate_message(content: &str, max_length: usize) -> Vec<String> {
    truncate_message_by_units(content, max_length, UnitMode::Codepoint)
}

pub fn truncate_message_utf16(content: &str, max_length: usize) -> Vec<String> {
    truncate_message_by_units(content, max_length, UnitMode::Utf16)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum UnitMode {
    Codepoint,
    Utf16,
}

fn truncate_message_by_units(content: &str, max_length: usize, mode: UnitMode) -> Vec<String> {
    if unit_len(content, mode) <= max_length {
        return vec![content.to_string()];
    }

    const INDICATOR_RESERVE: usize = 10;
    const FENCE_CLOSE: &str = "\n```";
    let mut chunks = Vec::new();
    let mut remaining = content.to_string();
    let mut carry_lang: Option<String> = None;

    while !remaining.is_empty() {
        let prefix = carry_lang
            .as_ref()
            .map(|lang| format!("```{}\n", lang))
            .unwrap_or_default();
        let mut headroom = max_length
            .saturating_sub(INDICATOR_RESERVE)
            .saturating_sub(unit_len(&prefix, mode))
            .saturating_sub(unit_len(FENCE_CLOSE, mode));
        if headroom < 1 {
            headroom = max_length / 2;
        }

        if unit_len(&prefix, mode) + unit_len(&remaining, mode)
            <= max_length.saturating_sub(INDICATOR_RESERVE)
        {
            chunks.push(format!("{}{}", prefix, remaining));
            break;
        }

        let cp_limit = unit_floor(&remaining, headroom, mode);
        let region = &remaining[..cp_limit];
        let mut split_at = region.rfind('\n').unwrap_or(0);
        if split_at < cp_limit / 2 {
            split_at = region.rfind(' ').unwrap_or(0);
        }
        if split_at < 1 {
            split_at = cp_limit;
        }

        let candidate = &remaining[..split_at];
        let backtick_count = unescaped_backtick_count(candidate);
        if backtick_count % 2 == 1 {
            if let Some(last_bt) = last_unescaped_backtick(candidate) {
                let safe_split = candidate[..last_bt]
                    .rfind(' ')
                    .into_iter()
                    .chain(candidate[..last_bt].rfind('\n'))
                    .max()
                    .unwrap_or(0);
                if safe_split > cp_limit / 4 {
                    split_at = safe_split;
                }
            }
        }

        let chunk_body = remaining[..split_at].to_string();
        remaining = remaining[split_at..].trim_start().to_string();
        let mut full_chunk = format!("{}{}", prefix, chunk_body);

        let mut in_code = carry_lang.is_some();
        let mut lang = carry_lang.clone().unwrap_or_default();
        for line in chunk_body.split('\n') {
            let stripped = line.trim();
            if stripped.starts_with("```") {
                if in_code {
                    in_code = false;
                    lang.clear();
                } else {
                    in_code = true;
                    let tag = stripped[3..].trim();
                    lang = tag.split_whitespace().next().unwrap_or("").to_string();
                }
            }
        }
        if in_code {
            full_chunk.push_str(FENCE_CLOSE);
            carry_lang = Some(lang);
        } else {
            carry_lang = None;
        }
        chunks.push(full_chunk);
    }

    if chunks.len() > 1 {
        let total = chunks.len();
        chunks = chunks
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| format!("{} ({}/{})", chunk, idx + 1, total))
            .collect();
    }

    chunks
}

fn unit_len(text: &str, mode: UnitMode) -> usize {
    match mode {
        UnitMode::Codepoint => text.chars().count(),
        UnitMode::Utf16 => text.encode_utf16().count(),
    }
}

fn unit_floor(text: &str, max_units: usize, mode: UnitMode) -> usize {
    if unit_len(text, mode) <= max_units {
        return text.len();
    }
    let mut used = 0usize;
    for (idx, ch) in text.char_indices() {
        let width = match mode {
            UnitMode::Codepoint => 1,
            UnitMode::Utf16 => ch.len_utf16(),
        };
        if used + width > max_units {
            return idx;
        }
        used += width;
    }
    text.len()
}

fn unescaped_backtick_count(text: &str) -> usize {
    text.as_bytes()
        .iter()
        .enumerate()
        .filter(|(idx, byte)| **byte == b'`' && (*idx == 0 || text.as_bytes()[idx - 1] != b'\\'))
        .count()
}

fn last_unescaped_backtick(text: &str) -> Option<usize> {
    text.as_bytes()
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, byte)| {
            if *byte == b'`' && (idx == 0 || text.as_bytes()[idx - 1] != b'\\') {
                Some(idx)
            } else {
                None
            }
        })
}

pub fn model_short(model: Option<&str>) -> String {
    model
        .unwrap_or("")
        .rsplit_once('/')
        .map(|(_, tail)| tail)
        .unwrap_or_else(|| model.unwrap_or(""))
        .to_string()
}

pub fn home_relative_cwd(cwd: Option<&str>, home: Option<&str>) -> String {
    let cwd = cwd.unwrap_or("");
    if cwd.is_empty() {
        return String::new();
    }
    let home = home.unwrap_or("");
    if !home.is_empty() && (cwd == home || cwd.starts_with(&format!("{}/", home))) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_string()
    }
}

pub fn format_runtime_footer(
    model: Option<&str>,
    context_tokens: i64,
    context_length: Option<i64>,
    cwd: Option<&str>,
    fields: &[&str],
    home: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    for field in fields {
        match *field {
            "model" => {
                let short = model_short(model);
                if !short.is_empty() {
                    parts.push(short);
                }
            }
            "context_pct" => {
                if let Some(length) = context_length {
                    if length > 0 && context_tokens >= 0 {
                        let pct = ((context_tokens as f64 / length as f64) * 100.0).round() as i64;
                        parts.push(pct.clamp(0, 100).to_string() + "%");
                    }
                }
            }
            "cwd" => {
                let relative = home_relative_cwd(cwd, home);
                if !relative.is_empty() {
                    parts.push(relative);
                }
            }
            _ => {}
        }
    }
    parts.join(" \u{00b7} ")
}

pub fn is_retryable_error(error: Option<&str>) -> bool {
    let Some(error) = error else {
        return false;
    };
    let lowered = error.to_lowercase();
    retryable_error_patterns()
        .iter()
        .any(|pattern| lowered.contains(pattern))
}

pub fn is_timeout_error(error: Option<&str>) -> bool {
    let Some(error) = error else {
        return false;
    };
    let lowered = error.to_lowercase();
    lowered.contains("timed out")
        || lowered.contains("readtimeout")
        || lowered.contains("writetimeout")
}

pub fn retryable_error_patterns() -> &'static [&'static str] {
    &[
        "connecterror",
        "connectionerror",
        "connectionreset",
        "connectionrefused",
        "connecttimeout",
        "network",
        "broken pipe",
        "remotedisconnected",
        "eoferror",
    ]
}

pub fn retry_decision(error: Option<&str>) -> RetryDecision {
    let retryable = is_retryable_error(error);
    let timeout = is_timeout_error(error);
    let action = if retryable {
        "retry"
    } else if timeout {
        "return_failure_no_retry"
    } else {
        "plain_text_fallback"
    };
    RetryDecision {
        retryable,
        timeout,
        action: action.to_string(),
    }
}

pub fn retry_decision_samples() -> BTreeMap<String, RetryDecision> {
    [
        "httpx.ConnectError: connection dropped",
        "Forbidden: bot was blocked by the user",
        "Bad Request: can't parse entities",
        "CONNECTERROR: host unreachable",
        "ReadTimeout: request timed out",
        "Timed out waiting for response",
        "ConnectTimeout: connection timed out",
        "internal platform error",
    ]
    .into_iter()
    .map(|error| (error.to_string(), retry_decision(Some(error))))
    .collect()
}

pub fn send_retry_plan_samples() -> BTreeMap<String, SendRetryPlan> {
    let network_err = SendResult {
        success: false,
        message_id: None,
        error: Some("httpx.ConnectError: host unreachable".to_string()),
        retryable: false,
    };
    BTreeMap::from([
        (
            "success_first_attempt".to_string(),
            plan_send_with_retry(vec![ok("123")], 2),
        ),
        (
            "connect_error_then_success".to_string(),
            plan_send_with_retry(vec![network_err.clone(), ok("ok")], 2),
        ),
        (
            "read_timeout_not_retried".to_string(),
            plan_send_with_retry(
                vec![SendResult {
                    success: false,
                    message_id: None,
                    error: Some("ReadTimeout: request timed out".to_string()),
                    retryable: false,
                }],
                3,
            ),
        ),
        (
            "retryable_flag_then_success".to_string(),
            plan_send_with_retry(
                vec![
                    SendResult {
                        success: false,
                        message_id: None,
                        error: Some("internal platform error".to_string()),
                        retryable: true,
                    },
                    ok("ok"),
                ],
                2,
            ),
        ),
        (
            "network_to_formatting_fallback".to_string(),
            plan_send_with_retry(
                vec![
                    network_err.clone(),
                    SendResult {
                        success: false,
                        message_id: None,
                        error: Some("Bad Request: can't parse entities".to_string()),
                        retryable: false,
                    },
                    ok("fallback_ok"),
                ],
                2,
            ),
        ),
        (
            "network_exhausted_notice".to_string(),
            plan_send_with_retry(
                vec![
                    network_err.clone(),
                    network_err.clone(),
                    network_err,
                    SendResult {
                        success: true,
                        message_id: None,
                        error: None,
                        retryable: false,
                    },
                ],
                2,
            ),
        ),
    ])
}

fn ok(message_id: &str) -> SendResult {
    SendResult {
        success: true,
        message_id: Some(message_id.to_string()),
        error: None,
        retryable: false,
    }
}

pub fn plan_send_with_retry(mut results: Vec<SendResult>, max_retries: usize) -> SendRetryPlan {
    let mut send_calls = 1usize;
    let mut result = pop_result(&mut results);
    if result.success {
        return send_retry_plan(result, send_calls, false, false);
    }

    let mut error_str = result.error.clone().unwrap_or_default();
    let is_network = result.retryable || is_retryable_error(Some(&error_str));

    if !is_network && is_timeout_error(Some(&error_str)) {
        return send_retry_plan(result, send_calls, false, false);
    }

    if is_network {
        let mut exhausted = true;
        for _attempt in 1..=max_retries {
            send_calls += 1;
            result = pop_result(&mut results);
            if result.success {
                return send_retry_plan(result, send_calls, false, false);
            }
            error_str = result.error.clone().unwrap_or_default();
            if !(result.retryable || is_retryable_error(Some(&error_str))) {
                exhausted = false;
                break;
            }
        }
        if exhausted {
            send_calls += 1;
            return send_retry_plan(result, send_calls, false, true);
        }
    }

    send_calls += 1;
    let fallback_result = pop_result(&mut results);
    send_retry_plan(fallback_result, send_calls, true, false)
}

fn pop_result(results: &mut Vec<SendResult>) -> SendResult {
    if results.is_empty() {
        SendResult {
            success: true,
            message_id: Some("ok".to_string()),
            error: None,
            retryable: false,
        }
    } else {
        results.remove(0)
    }
}

fn send_retry_plan(
    result: SendResult,
    send_calls: usize,
    fallback_sent: bool,
    notice_sent: bool,
) -> SendRetryPlan {
    SendRetryPlan {
        success: result.success,
        send_calls,
        message_id: result.message_id,
        final_error: result.error,
        fallback_sent,
        notice_sent,
    }
}

fn classify_gateway_action(canonical: &str, args: &str) -> String {
    match canonical {
        "approve" => {
            if args.trim() == "always" {
                "approval_allow_always"
            } else {
                "approval_allow_once"
            }
        }
        "deny" => "approval_deny",
        "yolo" => "toggle_yolo",
        "reload-mcp" => "reload_mcp",
        "reload-skills" => "reload_skills_deferred",
        "title" => "set_title",
        "resume" => "resume_session",
        "background" => {
            if args.trim().is_empty() {
                "usage_error"
            } else {
                "background_prompt"
            }
        }
        "queue" => {
            if args.trim().is_empty() {
                "usage_error"
            } else {
                "queue_prompt"
            }
        }
        "steer" => {
            if args.trim().is_empty() {
                "usage_error"
            } else {
                "steer_prompt"
            }
        }
        "status" => "status",
        "help" | "commands" => "help",
        "new" => "new_session",
        "stop" => "stop_or_interrupt",
        "profile" => "profile",
        "agents" => "agents_status",
        "restart" => "restart_gateway",
        "update" => "update",
        _ => "dispatch_or_busy",
    }
    .to_string()
}

pub fn smoke_session_guard_trace() -> GuardTrace {
    let mut guard = SessionGuard::new();
    let session_key = "agent:main:telegram:dm:42";
    guard.begin_turn(session_key);
    guard.enqueue_fifo(session_key, test_event("first"));
    let depth_after_first_enqueue = guard.queue_depth(session_key);
    guard.enqueue_fifo(session_key, test_event("second"));
    let depth_after_second_enqueue = guard.queue_depth(session_key);
    let first_pending_text = guard.pending_text(session_key).map(str::to_string);

    let drained = guard.dequeue_pending(session_key);
    let promoted = guard.promote_queued_event(session_key, drained);
    let active_during_drain = guard.is_active(session_key);
    let promoted_pending_text = promoted.map(|event| event.text);
    let staged_pending_text = guard.pending_text(session_key).map(str::to_string);
    let _ = guard.finish_turn(session_key);
    let _ = guard.finish_turn(session_key);
    let active_after_empty_finish = guard.is_active(session_key);

    guard.begin_turn(session_key);
    let stop_bypasses_busy_guard = matches!(
        guard.handle_busy_message(session_key, test_event("/stop"), "queue", Some("stop")),
        BusyDecision::ProcessCommand
    );
    let status_bypasses_busy_guard = matches!(
        guard.handle_busy_message(session_key, test_event("/status"), "queue", Some("status")),
        BusyDecision::ProcessCommand
    );
    let plain_message_queued = matches!(
        guard.handle_busy_message(session_key, test_event("plain"), "queue", None),
        BusyDecision::QueuePending
    );

    GuardTrace {
        depth_after_first_enqueue,
        depth_after_second_enqueue,
        first_pending_text,
        promoted_pending_text,
        staged_pending_text,
        active_during_drain,
        active_after_empty_finish,
        stop_bypasses_busy_guard,
        status_bypasses_busy_guard,
        plain_message_queued,
    }
}

fn test_event(text: &str) -> MessageEvent {
    MessageEvent {
        text: text.to_string(),
        message_type: "text".to_string(),
        platform: "telegram".to_string(),
        chat_id: "42".to_string(),
        user_id: None,
        message_id: None,
        thread_id: None,
        media_urls: vec![],
        media_types: vec![],
        internal: false,
        metadata: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_adapter_smokes_full_lifecycle() {
        let status = smoke_adapter_roundtrip().await.unwrap();
        assert_eq!(status.platform, "webhook");
        assert!(status.connected);
        assert!(status.started);
        assert_eq!(status.sent_count, 1);
        assert_eq!(status.pending_count, 0);
        assert_eq!(status.token_lock_scope.as_deref(), Some("webhook_token"));
    }

    #[tokio::test]
    async fn send_before_connect_returns_retryable_failure() {
        let mut adapter = InMemoryAdapter::new("local", "Local");
        let result = adapter
            .send(SendRequest {
                chat_id: "chat".to_string(),
                content: "hello".to_string(),
                reply_to: None,
                metadata: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.retryable);
    }

    #[test]
    fn token_lock_conflicts_are_non_retryable() {
        let mut adapter = InMemoryAdapter::new("telegram", "Telegram");
        adapter
            .acquire_token_lock(TokenLock {
                scope: "telegram_token".to_string(),
                identity: "a".to_string(),
                resource_desc: "Telegram bot token".to_string(),
            })
            .unwrap();
        let err = adapter
            .acquire_token_lock(TokenLock {
                scope: "telegram_token".to_string(),
                identity: "b".to_string(),
                resource_desc: "Telegram bot token".to_string(),
            })
            .unwrap_err();
        assert_eq!(err.code, "token_lock_conflict");
        assert!(!err.retryable);
    }

    #[test]
    fn session_guard_fifo_and_lifecycle_trace_matches_expected() {
        let trace = smoke_session_guard_trace();
        assert_eq!(trace.depth_after_first_enqueue, 1);
        assert_eq!(trace.depth_after_second_enqueue, 2);
        assert_eq!(trace.first_pending_text.as_deref(), Some("first"));
        assert_eq!(trace.promoted_pending_text.as_deref(), Some("first"));
        assert_eq!(trace.staged_pending_text.as_deref(), Some("second"));
        assert!(trace.active_during_drain);
        assert!(!trace.active_after_empty_finish);
        assert!(trace.stop_bypasses_busy_guard);
        assert!(trace.status_bypasses_busy_guard);
        assert!(trace.plain_message_queued);
    }

    #[test]
    fn gateway_command_routes_control_actions_and_aliases() {
        let samples = gateway_command_route_samples();
        assert_eq!(
            samples["/approve always"].as_ref().unwrap().action,
            "approval_allow_always"
        );
        assert_eq!(samples["/deny"].as_ref().unwrap().action, "approval_deny");
        assert_eq!(
            samples["/bg run report"].as_ref().unwrap().canonical_name,
            "background"
        );
        assert_eq!(
            samples["/bg run report"].as_ref().unwrap().action,
            "background_prompt"
        );
        assert!(samples["/status"].as_ref().unwrap().bypass_active_session);
        assert!(samples["/tools list"].is_none());
        assert!(samples["/unknown"].is_none());
    }

    #[test]
    fn streaming_delivery_snapshot_covers_fresh_final_and_cleanup() {
        let snapshot = streaming_delivery_snapshot();
        assert!(snapshot.truncated_plain.len() > 1);
        assert!(snapshot
            .truncated_code
            .iter()
            .any(|chunk| chunk.contains("```")));
        assert!(!snapshot.cleaned_display.contains("MEDIA:"));
        assert!(!snapshot.cleaned_display.contains("[[audio_as_voice]]"));
        assert_eq!(
            snapshot.thread_metadata.unwrap().get("thread_id").unwrap(),
            "topic-1"
        );
        assert!(snapshot.fresh_final_success.should_send_fresh_final);
        assert!(snapshot
            .fresh_final_success
            .calls
            .contains(&"delete_initial_preview".to_string()));
        assert!(snapshot.fresh_final_send_failure.fallback_to_edit);
        assert!(!snapshot.fresh_final_disabled.should_send_fresh_final);
    }
}

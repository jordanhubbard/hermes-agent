//! Production-shaped Rust agent runtime.
//!
//! This module wires the core loop to replaceable provider, tool, store, and
//! hook boundaries. It is intentionally synchronous like the Python
//! `AIAgent.run_conversation` loop, but unlike the older canned replay it can
//! call a real provider implementation, dispatch real tools through a trait,
//! persist messages, apply fallback policy, and emit lifecycle hooks.

use serde::{Deserialize, Serialize};

use crate::budget::{ConversationBudget, TokenUsage};
use crate::compression::{CompressionEvent, CompressionTrigger};
use crate::compression_plan::{plan_compression, CompressionPlanOptions};
use crate::message::{Message, ToolTurn};
use crate::outcome::{ConversationOutcome, InterruptKind};
use crate::provider::ProviderRouting;
use crate::provider_http::{
    execute_provider_request, ProviderHttpError, ProviderHttpOptions, ProviderHttpResponse,
};
use crate::provider_wire::{ParsedProviderResponse, ProviderErrorClass, ProviderRequestOptions};
use crate::tool::{ToolCall, ToolDefinition, ToolResult};

/// Runtime configuration for one agent turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRuntimeConfig {
    /// Maximum model calls that may include tool calls before the loop stops.
    pub max_iterations: u32,
    /// Budget snapshot carried into the run.
    #[serde(default)]
    pub budget: ConversationBudget,
    /// Interrupt before a model call at this zero-based index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupt_before_model_call: Option<u32>,
    /// Compression behavior when the context budget is exhausted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<RuntimeCompressionOptions>,
    /// Request-level provider options.
    #[serde(default)]
    pub request_options: ProviderRequestOptions,
    /// Provider error classes that should try the fallback client once.
    #[serde(default)]
    pub fallback_on: Vec<ProviderErrorClass>,
}

impl Default for AgentRuntimeConfig {
    fn default() -> Self {
        Self {
            max_iterations: 90,
            budget: ConversationBudget::default(),
            interrupt_before_model_call: None,
            compression: None,
            request_options: ProviderRequestOptions::default(),
            fallback_on: vec![ProviderErrorClass::RateLimit, ProviderErrorClass::Transient],
        }
    }
}

/// Compression configuration for the live runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCompressionOptions {
    /// Session ID before compression.
    pub parent_session_id: String,
    /// Session ID for the continuation.
    pub child_session_id: String,
    /// Messages preserved from the head of the transcript.
    pub head_messages: usize,
    /// Messages preserved from the tail of the transcript.
    pub tail_messages: usize,
    /// Summary text produced by a caller-owned summarizer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Result of one runtime execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRuntimeResult {
    /// Final outcome.
    pub outcome: ConversationOutcome,
    /// Final budget snapshot.
    pub budget: ConversationBudget,
    /// Transcript at the end of the run.
    pub messages: Vec<Message>,
    /// Number of model calls attempted.
    pub model_call_count: u32,
    /// Number of tool iterations completed.
    pub tool_iteration_count: u32,
    /// Number of tool calls dispatched.
    pub tool_call_count: u32,
    /// Whether the fallback model client was used.
    pub fallback_used: bool,
    /// Compression events applied during the run.
    pub compression_events: Vec<CompressionEvent>,
}

/// Provider boundary used by the runtime.
pub trait ModelClient {
    /// Execute one model call and return the normalized response.
    fn call(
        &mut self,
        messages: &[Message],
        tools: &[ToolDefinition],
        options: &ProviderRequestOptions,
    ) -> Result<ParsedProviderResponse, ProviderHttpError>;
}

/// HTTP-backed model client using `provider_http`.
#[derive(Debug, Clone)]
pub struct HttpModelClient {
    /// Resolved provider routing.
    pub routing: ProviderRouting,
    /// HTTP options.
    pub http_options: ProviderHttpOptions,
}

impl ModelClient for HttpModelClient {
    fn call(
        &mut self,
        messages: &[Message],
        tools: &[ToolDefinition],
        options: &ProviderRequestOptions,
    ) -> Result<ParsedProviderResponse, ProviderHttpError> {
        execute_provider_request(messages, tools, &self.routing, options, &self.http_options)
            .map(|response: ProviderHttpResponse| response.parsed)
    }
}

/// Tool execution boundary used by the runtime.
pub trait ToolDispatcher {
    /// Execute one tool call.
    fn dispatch(&mut self, call: &ToolCall) -> ToolResult;
}

/// Persistence boundary used by the runtime.
pub trait ConversationStore {
    /// Persist one message.
    fn persist_message(&mut self, message: &Message);
    /// Persist one compression event.
    fn persist_compression(&mut self, event: &CompressionEvent);
    /// Persist final token/accounting totals.
    fn persist_token_update(&mut self, _budget: &ConversationBudget, _model_call_count: u32) {}
}

/// Lifecycle hook boundary used by the runtime.
pub trait RuntimeHooks {
    /// Session is starting.
    fn on_session_start(&mut self) {}
    /// Model call is about to start.
    fn on_model_call(&mut self, _call_index: u32, _message_count: usize) {}
    /// Provider returned an error.
    fn on_provider_error(&mut self, _error: &ProviderHttpError) {}
    /// Tool call is about to start.
    fn on_tool_call(&mut self, _call: &ToolCall) {}
    /// Session ended.
    fn on_session_end(&mut self, _outcome: &ConversationOutcome) {}
}

/// No-op store for callers that do not persist.
#[derive(Debug, Default)]
pub struct NoopStore;

impl ConversationStore for NoopStore {
    fn persist_message(&mut self, _message: &Message) {}
    fn persist_compression(&mut self, _event: &CompressionEvent) {}
}

/// No-op hook receiver.
#[derive(Debug, Default)]
pub struct NoopHooks;

impl RuntimeHooks for NoopHooks {}

/// Runtime dependency bundle.
pub struct RuntimeDeps<'a> {
    /// Primary model client.
    pub model: &'a mut dyn ModelClient,
    /// Optional fallback model client.
    pub fallback_model: Option<&'a mut dyn ModelClient>,
    /// Tool dispatcher.
    pub tools: &'a mut dyn ToolDispatcher,
    /// Persistence boundary.
    pub store: &'a mut dyn ConversationStore,
    /// Lifecycle hooks.
    pub hooks: &'a mut dyn RuntimeHooks,
}

/// Run one production-shaped conversation loop.
pub fn run_agent_runtime(
    mut messages: Vec<Message>,
    tool_definitions: &[ToolDefinition],
    mut config: AgentRuntimeConfig,
    mut deps: RuntimeDeps<'_>,
) -> AgentRuntimeResult {
    deps.hooks.on_session_start();
    for message in &messages {
        deps.store.persist_message(message);
    }

    let mut model_call_count = 0_u32;
    let mut tool_iteration_count = 0_u32;
    let mut tool_call_count = 0_u32;
    let mut fallback_used = false;
    let mut compression_events = Vec::new();

    loop {
        if config.budget.turns_exhausted() {
            return finish(
                ConversationOutcome::Interrupted {
                    reason: InterruptKind::MaxTurns,
                    detail: Some("turn budget exhausted".to_string()),
                },
                config.budget,
                messages,
                model_call_count,
                tool_iteration_count,
                tool_call_count,
                fallback_used,
                compression_events,
                deps,
            );
        }

        if config.budget.context_exhausted() {
            match apply_runtime_compression(&mut messages, &config, deps.store) {
                Some(event) => {
                    compression_events.push(event);
                    config.budget.usage = TokenUsage::default();
                }
                None => {
                    return finish(
                        ConversationOutcome::ContextOverflow,
                        config.budget,
                        messages,
                        model_call_count,
                        tool_iteration_count,
                        tool_call_count,
                        fallback_used,
                        compression_events,
                        deps,
                    )
                }
            }
        }

        if config.interrupt_before_model_call == Some(model_call_count) {
            return finish(
                ConversationOutcome::Interrupted {
                    reason: InterruptKind::User,
                    detail: Some("interrupt flag set before model call".to_string()),
                },
                config.budget,
                messages,
                model_call_count,
                tool_iteration_count,
                tool_call_count,
                fallback_used,
                compression_events,
                deps,
            );
        }

        if tool_iteration_count >= config.max_iterations {
            return finish(
                ConversationOutcome::Interrupted {
                    reason: InterruptKind::MaxTurns,
                    detail: Some("max tool-call iterations reached".to_string()),
                },
                config.budget,
                messages,
                model_call_count,
                tool_iteration_count,
                tool_call_count,
                fallback_used,
                compression_events,
                deps,
            );
        }

        deps.hooks.on_model_call(model_call_count, messages.len());
        model_call_count = model_call_count.saturating_add(1);
        let response = match deps
            .model
            .call(&messages, tool_definitions, &config.request_options)
        {
            Ok(response) => response,
            Err(error) => {
                deps.hooks.on_provider_error(&error);
                if should_try_fallback(&config, &error, fallback_used) {
                    if let Some(fallback) = deps.fallback_model.as_deref_mut() {
                        fallback_used = true;
                        deps.hooks.on_model_call(model_call_count, messages.len());
                        model_call_count = model_call_count.saturating_add(1);
                        match fallback.call(&messages, tool_definitions, &config.request_options) {
                            Ok(response) => response,
                            Err(fallback_error) => {
                                return finish(
                                    ConversationOutcome::ProviderError {
                                        error: fallback_error.message,
                                    },
                                    config.budget,
                                    messages,
                                    model_call_count,
                                    tool_iteration_count,
                                    tool_call_count,
                                    fallback_used,
                                    compression_events,
                                    deps,
                                )
                            }
                        }
                    } else {
                        return finish_provider_error(
                            error,
                            config.budget,
                            messages,
                            model_call_count,
                            tool_iteration_count,
                            tool_call_count,
                            fallback_used,
                            compression_events,
                            deps,
                        );
                    }
                } else {
                    return finish_provider_error(
                        error,
                        config.budget,
                        messages,
                        model_call_count,
                        tool_iteration_count,
                        tool_call_count,
                        fallback_used,
                        compression_events,
                        deps,
                    );
                }
            }
        };
        config.budget.usage.add(&response.usage);
        config.budget.turn_count = config.budget.turn_count.saturating_add(1);

        let tool_calls = response.assistant.tool_calls.clone();
        let final_text = response.assistant.content.clone();
        let assistant_message = Message::Assistant(response.assistant);
        deps.store.persist_message(&assistant_message);
        messages.push(assistant_message);

        if tool_calls.is_empty() {
            return finish(
                ConversationOutcome::Completed {
                    final_message: final_text.unwrap_or_default(),
                },
                config.budget,
                messages,
                model_call_count,
                tool_iteration_count,
                tool_call_count,
                fallback_used,
                compression_events,
                deps,
            );
        }

        for call in tool_calls {
            deps.hooks.on_tool_call(&call);
            let result = deps.tools.dispatch(&call);
            let tool_message = Message::Tool(ToolTurn {
                tool_call_id: call.id,
                name: Some(call.name),
                content: result.content,
                ok: Some(result.ok),
            });
            deps.store.persist_message(&tool_message);
            messages.push(tool_message);
            tool_call_count = tool_call_count.saturating_add(1);
        }
        tool_iteration_count = tool_iteration_count.saturating_add(1);
    }
}

fn should_try_fallback(
    config: &AgentRuntimeConfig,
    error: &ProviderHttpError,
    fallback_used: bool,
) -> bool {
    !fallback_used && config.fallback_on.contains(&error.class)
}

fn finish_provider_error(
    error: ProviderHttpError,
    budget: ConversationBudget,
    messages: Vec<Message>,
    model_call_count: u32,
    tool_iteration_count: u32,
    tool_call_count: u32,
    fallback_used: bool,
    compression_events: Vec<CompressionEvent>,
    deps: RuntimeDeps<'_>,
) -> AgentRuntimeResult {
    finish(
        ConversationOutcome::ProviderError {
            error: error.message,
        },
        budget,
        messages,
        model_call_count,
        tool_iteration_count,
        tool_call_count,
        fallback_used,
        compression_events,
        deps,
    )
}

fn finish(
    outcome: ConversationOutcome,
    budget: ConversationBudget,
    messages: Vec<Message>,
    model_call_count: u32,
    tool_iteration_count: u32,
    tool_call_count: u32,
    fallback_used: bool,
    compression_events: Vec<CompressionEvent>,
    deps: RuntimeDeps<'_>,
) -> AgentRuntimeResult {
    deps.store.persist_token_update(&budget, model_call_count);
    deps.hooks.on_session_end(&outcome);
    AgentRuntimeResult {
        outcome,
        budget,
        messages,
        model_call_count,
        tool_iteration_count,
        tool_call_count,
        fallback_used,
        compression_events,
    }
}

fn apply_runtime_compression(
    messages: &mut Vec<Message>,
    config: &AgentRuntimeConfig,
    store: &mut dyn ConversationStore,
) -> Option<CompressionEvent> {
    let options = config.compression.as_ref()?;
    let plan = plan_compression(
        messages,
        CompressionPlanOptions {
            parent_session_id: options.parent_session_id.clone(),
            child_session_id: options.child_session_id.clone(),
            trigger: CompressionTrigger::ContextLimit,
            head_messages: options.head_messages,
            tail_messages: options.tail_messages,
            summary: options.summary.clone(),
            usage_at_trigger: config.budget.usage,
            provider_error: None,
        },
    );
    store.persist_compression(&plan.event);
    for message in &plan.retained_messages {
        store.persist_message(message);
    }
    *messages = plan.retained_messages;
    Some(plan.event)
}

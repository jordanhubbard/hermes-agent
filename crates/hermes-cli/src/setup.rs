use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProviderSetupDef {
    pub id: &'static str,
    pub name: &'static str,
    pub transport: &'static str,
    pub api_key_env_vars: &'static [&'static str],
    pub base_url: &'static str,
    pub base_url_env_var: &'static str,
    pub is_aggregator: bool,
    pub auth_type: &'static str,
    pub source: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SecretStoragePlan {
    pub provider: String,
    pub secret_store: String,
    pub secret_targets: Vec<String>,
    pub config_keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ModelChoicePlan {
    pub provider: String,
    pub model: String,
    pub config: Value,
    pub config_keys_written: Vec<String>,
    pub secret_keys_written_to_config: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SetupSnapshot {
    pub providers: BTreeMap<String, ProviderSetupDef>,
    pub aliases: BTreeMap<String, String>,
    pub api_modes: BTreeMap<String, String>,
    pub same_provider_pool_support: BTreeMap<String, bool>,
    pub secret_storage: BTreeMap<String, SecretStoragePlan>,
    pub model_choice_plans: BTreeMap<String, ModelChoicePlan>,
    pub auth_command_choices: BTreeMap<String, Vec<String>>,
}

pub fn setup_snapshot() -> SetupSnapshot {
    let provider_ids = [
        "openrouter",
        "nous",
        "openai-codex",
        "anthropic",
        "lmstudio",
        "copilot-acp",
        "bedrock",
        "gemini",
        "zai",
    ];
    SetupSnapshot {
        providers: provider_ids
            .into_iter()
            .filter_map(|id| provider_setup_def(id).map(|def| (id.to_string(), def)))
            .collect(),
        aliases: provider_alias_samples(),
        api_modes: api_mode_samples(),
        same_provider_pool_support: [
            "openrouter",
            "nous",
            "anthropic",
            "openai-codex",
            "custom",
            "copilot-acp",
            "bedrock",
        ]
        .into_iter()
        .map(|provider| {
            (
                provider.to_string(),
                supports_same_provider_pool_setup(provider),
            )
        })
        .collect(),
        secret_storage: [
            "openrouter",
            "anthropic",
            "nous",
            "openai-codex",
            "custom",
            "copilot-acp",
            "bedrock",
        ]
        .into_iter()
        .map(|provider| (provider.to_string(), secret_storage_plan(provider)))
        .collect(),
        model_choice_plans: model_choice_plan_samples(),
        auth_command_choices: auth_command_choices(),
    }
}

pub fn provider_setup_def(provider: &str) -> Option<ProviderSetupDef> {
    match normalize_provider(provider).as_str() {
        "openrouter" => Some(ProviderSetupDef {
            id: "openrouter",
            name: "OpenRouter",
            transport: "openai_chat",
            api_key_env_vars: &["OPENROUTER_API_KEY", "OPENAI_API_KEY"],
            base_url: "https://openrouter.ai/api/v1",
            base_url_env_var: "OPENROUTER_BASE_URL",
            is_aggregator: true,
            auth_type: "api_key",
            source: "models.dev",
        }),
        "nous" => Some(ProviderSetupDef {
            id: "nous",
            name: "Nous Portal",
            transport: "openai_chat",
            api_key_env_vars: &[],
            base_url: "https://inference-api.nousresearch.com/v1",
            base_url_env_var: "",
            is_aggregator: false,
            auth_type: "oauth_device_code",
            source: "hermes",
        }),
        "openai-codex" => Some(ProviderSetupDef {
            id: "openai-codex",
            name: "OpenAI",
            transport: "codex_responses",
            api_key_env_vars: &["OPENAI_API_KEY"],
            base_url: "https://chatgpt.com/backend-api/codex",
            base_url_env_var: "",
            is_aggregator: false,
            auth_type: "oauth_external",
            source: "models.dev",
        }),
        "anthropic" => Some(ProviderSetupDef {
            id: "anthropic",
            name: "Anthropic",
            transport: "anthropic_messages",
            api_key_env_vars: &[
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
            ],
            base_url: "",
            base_url_env_var: "",
            is_aggregator: false,
            auth_type: "api_key",
            source: "models.dev",
        }),
        "lmstudio" => Some(ProviderSetupDef {
            id: "lmstudio",
            name: "LMStudio",
            transport: "openai_chat",
            api_key_env_vars: &["LMSTUDIO_API_KEY", "LM_API_KEY"],
            base_url: "http://127.0.0.1:1234/v1",
            base_url_env_var: "LM_BASE_URL",
            is_aggregator: false,
            auth_type: "api_key",
            source: "models.dev",
        }),
        "copilot-acp" => Some(ProviderSetupDef {
            id: "copilot-acp",
            name: "GitHub Copilot ACP",
            transport: "codex_responses",
            api_key_env_vars: &[],
            base_url: "acp://copilot",
            base_url_env_var: "COPILOT_ACP_BASE_URL",
            is_aggregator: false,
            auth_type: "external_process",
            source: "hermes",
        }),
        "bedrock" => Some(ProviderSetupDef {
            id: "bedrock",
            name: "AWS Bedrock",
            transport: "bedrock_converse",
            api_key_env_vars: &[],
            base_url: "",
            base_url_env_var: "",
            is_aggregator: false,
            auth_type: "aws_sdk",
            source: "hermes",
        }),
        "gemini" => Some(ProviderSetupDef {
            id: "gemini",
            name: "Google",
            transport: "openai_chat",
            api_key_env_vars: &["GOOGLE_GENERATIVE_AI_API_KEY", "GEMINI_API_KEY"],
            base_url: "",
            base_url_env_var: "",
            is_aggregator: false,
            auth_type: "api_key",
            source: "models.dev",
        }),
        "zai" => Some(ProviderSetupDef {
            id: "zai",
            name: "Z.AI",
            transport: "openai_chat",
            api_key_env_vars: &[
                "ZHIPU_API_KEY",
                "GLM_API_KEY",
                "ZAI_API_KEY",
                "Z_AI_API_KEY",
            ],
            base_url: "https://api.z.ai/api/paas/v4",
            base_url_env_var: "GLM_BASE_URL",
            is_aggregator: false,
            auth_type: "api_key",
            source: "models.dev",
        }),
        _ => None,
    }
}

pub fn normalize_provider(name: &str) -> String {
    let key = name.trim().to_lowercase();
    match key.as_str() {
        "openai" => "openrouter",
        "claude" | "claude-code" => "anthropic",
        "github-copilot-acp" | "copilot-acp-agent" => "copilot-acp",
        "github" | "github-copilot" | "github-models" | "github-model" => "copilot",
        "glm" | "z-ai" | "z.ai" | "zhipu" => "zai",
        "aws" | "aws-bedrock" | "amazon-bedrock" | "amazon" => "bedrock",
        "lm-studio" | "lm_studio" => "lmstudio",
        "ollama" | "vllm" | "llamacpp" | "llama.cpp" | "llama-cpp" => "custom",
        other => other,
    }
    .to_string()
}

pub fn determine_api_mode(provider: &str, base_url: &str) -> String {
    let normalized = normalize_provider(provider);
    let url = base_url.trim().trim_end_matches('/').to_lowercase();
    if !url.is_empty() {
        if url.contains("api.kimi.com/coding")
            || url.ends_with("/anthropic")
            || url.contains("api.anthropic.com")
        {
            return "anthropic_messages".to_string();
        }
        if url.contains("api.openai.com") {
            return "codex_responses".to_string();
        }
        if url.contains("bedrock-runtime.") && url.contains("amazonaws.com") {
            return "bedrock_converse".to_string();
        }
    }
    match provider_setup_def(&normalized).map(|def| def.transport) {
        Some("anthropic_messages") => "anthropic_messages",
        Some("codex_responses") => "codex_responses",
        Some("bedrock_converse") => "bedrock_converse",
        _ => "chat_completions",
    }
    .to_string()
}

pub fn supports_same_provider_pool_setup(provider: &str) -> bool {
    let provider = normalize_provider(provider);
    if provider.is_empty() || provider == "custom" {
        return false;
    }
    if provider == "openrouter" {
        return true;
    }
    matches!(
        provider_setup_def(&provider).map(|def| def.auth_type),
        Some("api_key" | "oauth_device_code")
    )
}

pub fn secret_storage_plan(provider: &str) -> SecretStoragePlan {
    let provider = normalize_provider(provider);
    let secret_targets = provider_setup_def(&provider)
        .map(|def| {
            def.api_key_env_vars
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let secret_store = match provider_setup_def(&provider).map(|def| def.auth_type) {
        Some("oauth_device_code" | "oauth_external") => "auth.json",
        Some("external_process" | "aws_sdk") => "external",
        _ if secret_targets.is_empty() => "none",
        _ => ".env",
    };
    SecretStoragePlan {
        provider,
        secret_store: secret_store.to_string(),
        secret_targets,
        config_keys: vec![
            "model.provider".to_string(),
            "model.default".to_string(),
            "model.base_url".to_string(),
            "model.api_mode".to_string(),
        ],
    }
}

pub fn apply_model_choice(
    mut config: Value,
    provider: &str,
    model_name: &str,
    base_url: Option<&str>,
    api_mode: Option<&str>,
) -> ModelChoicePlan {
    let provider = normalize_provider(provider);
    if !config.is_object() {
        config = Value::Object(Map::new());
    }
    let root = config.as_object_mut().expect("object ensured");
    let existing_model = root.remove("model");
    let mut model = match existing_model {
        Some(Value::Object(map)) => map,
        Some(Value::String(text)) if !text.trim().is_empty() => {
            let mut map = Map::new();
            map.insert("default".to_string(), Value::String(text));
            map
        }
        _ => Map::new(),
    };
    model.insert("provider".to_string(), Value::String(provider.clone()));
    if !model_name.trim().is_empty() {
        model.insert(
            "default".to_string(),
            Value::String(model_name.trim().to_string()),
        );
    }
    match base_url.map(str::trim).filter(|value| !value.is_empty()) {
        Some(url) => {
            model.insert("base_url".to_string(), Value::String(url.to_string()));
        }
        None => {
            model.remove("base_url");
        }
    }
    match api_mode.map(str::trim).filter(|value| !value.is_empty()) {
        Some(mode) => {
            model.insert("api_mode".to_string(), Value::String(mode.to_string()));
        }
        None => {
            model.remove("api_mode");
        }
    }
    root.insert("model".to_string(), Value::Object(model));

    ModelChoicePlan {
        provider,
        model: model_name.to_string(),
        config,
        config_keys_written: vec![
            "model.provider".to_string(),
            "model.default".to_string(),
            "model.base_url".to_string(),
            "model.api_mode".to_string(),
        ],
        secret_keys_written_to_config: vec![],
    }
}

fn provider_alias_samples() -> BTreeMap<String, String> {
    [
        "openai",
        "claude",
        "github-copilot-acp",
        "glm",
        "google",
        "aws-bedrock",
        "lm-studio",
        "ollama",
    ]
    .into_iter()
    .map(|alias| (alias.to_string(), normalize_provider(alias)))
    .collect()
}

fn api_mode_samples() -> BTreeMap<String, String> {
    [
        ("openrouter", ""),
        ("anthropic", ""),
        ("openai-codex", ""),
        ("bedrock", ""),
        ("custom", "https://api.openai.com/v1"),
        ("custom-anthropic", "https://proxy.example/anthropic"),
        ("custom-kimi", "https://api.kimi.com/coding"),
        (
            "custom-bedrock",
            "https://bedrock-runtime.us-east-1.amazonaws.com",
        ),
    ]
    .into_iter()
    .map(|(provider, url)| {
        let actual_provider = provider.strip_prefix("custom-").unwrap_or(provider);
        (
            provider.to_string(),
            determine_api_mode(actual_provider, url),
        )
    })
    .collect()
}

fn model_choice_plan_samples() -> BTreeMap<String, ModelChoicePlan> {
    let base = serde_json::json!({
        "terminal": {"timeout": 999},
        "display": {"skin": "mono"},
        "model": {
            "default": "old-model",
            "provider": "custom",
            "base_url": "http://localhost:11434/v1",
            "api_mode": "chat_completions"
        }
    });
    BTreeMap::from([
        (
            "switch_custom_to_codex".to_string(),
            apply_model_choice(
                base.clone(),
                "openai-codex",
                "gpt-5.3-codex",
                Some("https://api.openai.com/v1"),
                Some("codex_responses"),
            ),
        ),
        (
            "switch_to_openrouter_preserves_other_config".to_string(),
            apply_model_choice(
                base,
                "openrouter",
                "anthropic/claude-opus-4.6",
                Some("https://openrouter.ai/api/v1"),
                None,
            ),
        ),
        (
            "string_model_becomes_dict".to_string(),
            apply_model_choice(
                serde_json::json!({"model": "legacy-model", "terminal": {"timeout": 50}}),
                "zai",
                "glm-5",
                Some("https://api.z.ai/api/paas/v4"),
                None,
            ),
        ),
    ])
}

fn auth_command_choices() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "login_providers".to_string(),
            vec!["nous".to_string(), "openai-codex".to_string()],
        ),
        (
            "logout_providers".to_string(),
            vec![
                "nous".to_string(),
                "openai-codex".to_string(),
                "spotify".to_string(),
            ],
        ),
        (
            "auth_subcommands".to_string(),
            vec![
                "add".to_string(),
                "list".to_string(),
                "remove".to_string(),
                "reset".to_string(),
                "status".to_string(),
                "logout".to_string(),
                "spotify".to_string(),
            ],
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choice_preserves_unrelated_config_and_keeps_secrets_out() {
        let plan = apply_model_choice(
            serde_json::json!({"terminal": {"timeout": 7}, "model": "old"}),
            "openai",
            "gpt-5.4",
            Some("https://openrouter.ai/api/v1"),
            None,
        );
        assert_eq!(plan.provider, "openrouter");
        assert_eq!(plan.config["terminal"]["timeout"], 7);
        assert_eq!(plan.config["model"]["default"], "gpt-5.4");
        assert!(plan.secret_keys_written_to_config.is_empty());
    }

    #[test]
    fn setup_snapshot_covers_auth_boundaries() {
        let snapshot = setup_snapshot();
        assert!(snapshot.same_provider_pool_support["openrouter"]);
        assert!(!snapshot.same_provider_pool_support["copilot-acp"]);
        assert_eq!(snapshot.secret_storage["openrouter"].secret_store, ".env");
        assert_eq!(
            snapshot.secret_storage["openai-codex"].secret_store,
            "auth.json"
        );
    }
}

//! Provider credential resolution.
//!
//! The Python runtime accepts credentials from explicit constructor values,
//! provider-specific environment variables, and same-provider credential pools.
//! This module keeps that precedence in Rust without reading Python code at
//! runtime.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::provider_http::ProviderHttpOptions;

/// A credential candidate from a configured same-provider pool.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PooledCredential {
    /// Provider this credential belongs to.
    pub provider: String,
    /// Secret API key.
    pub api_key: String,
    /// Optional provider base URL override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Human-readable pool label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Lower values are preferred when multiple credentials match.
    #[serde(default)]
    pub priority: i32,
    /// Disabled pool entries are ignored.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl fmt::Debug for PooledCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PooledCredential")
            .field("provider", &self.provider)
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("label", &self.label)
            .field("priority", &self.priority)
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl PooledCredential {
    /// Create an enabled pool credential with default priority.
    pub fn new(provider: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            api_key: api_key.into(),
            base_url: None,
            label: None,
            priority: 0,
            enabled: true,
        }
    }

    /// Set the pool label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the priority. Lower priority values are tried first.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set the base URL override.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Enable or disable the credential.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Input to credential resolution.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialRequest {
    /// Provider name or alias.
    pub provider: String,
    /// Explicit API key from caller/config. This wins over env and pools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_api_key: Option<String>,
    /// Explicit base URL from caller/config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_base_url: Option<String>,
    /// Environment snapshot. Tests pass a deterministic map; production can
    /// use [`CredentialRequest::from_process_env`].
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional env var order override. When empty, provider defaults are used.
    #[serde(default)]
    pub env_keys: Vec<String>,
    /// Same-provider credential pool candidates.
    #[serde(default)]
    pub credential_pool: Vec<PooledCredential>,
}

impl CredentialRequest {
    /// Build a request with the current process environment.
    pub fn from_process_env(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            env: std::env::vars().collect(),
            ..Self::default()
        }
    }

    /// Build a request from an explicit environment map.
    pub fn with_env(
        provider: impl Into<String>,
        env: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        Self {
            provider: provider.into(),
            env: env.into_iter().collect(),
            ..Self::default()
        }
    }
}

/// Where the selected credential came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CredentialSource {
    /// Explicit constructor/config value.
    Explicit,
    /// Provider environment variable.
    Env {
        /// Environment variable name.
        variable: String,
    },
    /// Same-provider pool entry.
    Pool {
        /// Optional pool label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

/// Resolved provider credential.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCredential {
    /// Normalized provider name.
    pub provider: String,
    /// Secret API key.
    pub api_key: String,
    /// Optional provider base URL override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Credential source.
    pub source: CredentialSource,
}

impl fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCredential")
            .field("provider", &self.provider)
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("source", &self.source)
            .finish()
    }
}

impl ResolvedCredential {
    /// Convert the credential into HTTP options for provider calls.
    pub fn http_options(&self, timeout_secs: Option<u64>) -> ProviderHttpOptions {
        ProviderHttpOptions {
            api_key: Some(self.api_key.clone()),
            timeout_secs: timeout_secs
                .unwrap_or_else(|| ProviderHttpOptions::default().timeout_secs),
        }
    }
}

/// Failure to resolve a usable credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialResolveError {
    /// Normalized provider name.
    pub provider: String,
    /// Env vars inspected.
    pub tried_env_keys: Vec<String>,
    /// Number of pool candidates inspected.
    pub pool_candidates: usize,
}

impl fmt::Display for CredentialResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "no credential resolved for provider {} (checked env vars: {}, pool candidates: {})",
            self.provider,
            self.tried_env_keys.join(", "),
            self.pool_candidates
        )
    }
}

impl std::error::Error for CredentialResolveError {}

/// Resolve a provider credential using Python-compatible precedence:
/// explicit value, provider env vars, then same-provider pool.
pub fn resolve_credential(
    request: &CredentialRequest,
) -> Result<ResolvedCredential, CredentialResolveError> {
    let provider = normalize_provider(&request.provider);
    let base_url = non_empty(request.explicit_base_url.as_deref()).map(ToOwned::to_owned);

    if let Some(api_key) = non_empty(request.explicit_api_key.as_deref()) {
        return Ok(ResolvedCredential {
            provider,
            api_key: api_key.to_string(),
            base_url,
            source: CredentialSource::Explicit,
        });
    }

    let env_keys = env_keys_for_request(request);
    for key in &env_keys {
        if let Some(api_key) = request
            .env
            .get(key)
            .and_then(|value| non_empty(Some(value)))
        {
            return Ok(ResolvedCredential {
                provider,
                api_key: api_key.to_string(),
                base_url,
                source: CredentialSource::Env {
                    variable: key.clone(),
                },
            });
        }
    }

    let pool_match = request
        .credential_pool
        .iter()
        .enumerate()
        .filter(|(_, credential)| {
            credential.enabled
                && normalize_provider(&credential.provider) == provider
                && non_empty(Some(&credential.api_key)).is_some()
        })
        .min_by_key(|(index, credential)| (credential.priority, *index));

    if let Some((_, credential)) = pool_match {
        return Ok(ResolvedCredential {
            provider,
            api_key: credential.api_key.clone(),
            base_url: base_url.or_else(|| credential.base_url.clone()),
            source: CredentialSource::Pool {
                label: credential.label.clone(),
            },
        });
    }

    Err(CredentialResolveError {
        provider,
        tried_env_keys: env_keys,
        pool_candidates: request.credential_pool.len(),
    })
}

/// Normalize provider aliases used by setup/config flows.
pub fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" => "openrouter".to_string(),
        "claude" | "claude-code" => "anthropic".to_string(),
        "github-copilot-acp" | "copilot-acp-agent" => "copilot-acp".to_string(),
        "google" | "google-ai" => "gemini".to_string(),
        other => other.to_string(),
    }
}

/// Provider-specific API key env vars in lookup order.
pub fn default_env_keys(provider: &str) -> Vec<String> {
    match normalize_provider(provider).as_str() {
        "openrouter" => vec!["OPENROUTER_API_KEY", "OPENAI_API_KEY"],
        "openai-codex" => vec!["OPENAI_API_KEY"],
        "anthropic" => vec![
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ],
        "lmstudio" => vec!["LMSTUDIO_API_KEY", "LM_API_KEY"],
        "gemini" => vec!["GOOGLE_GENERATIVE_AI_API_KEY", "GEMINI_API_KEY"],
        "zai" => vec![
            "ZHIPU_API_KEY",
            "GLM_API_KEY",
            "ZAI_API_KEY",
            "Z_AI_API_KEY",
        ],
        _ => Vec::new(),
    }
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn env_keys_for_request(request: &CredentialRequest) -> Vec<String> {
    if request.env_keys.is_empty() {
        default_env_keys(&request.provider)
    } else {
        request.env_keys.clone()
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn default_enabled() -> bool {
    true
}

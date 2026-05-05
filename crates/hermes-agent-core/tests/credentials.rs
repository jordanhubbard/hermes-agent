use hermes_agent_core::{
    resolve_credential, CredentialRequest, CredentialSource, PooledCredential,
};
use std::collections::BTreeMap;

#[test]
fn explicit_credential_wins_over_env_and_pool() {
    let request = CredentialRequest {
        provider: "anthropic".to_string(),
        explicit_api_key: Some("explicit-key".to_string()),
        env: BTreeMap::from([("ANTHROPIC_API_KEY".to_string(), "env-key".to_string())]),
        credential_pool: vec![PooledCredential::new("anthropic", "pool-key")],
        ..CredentialRequest::default()
    };

    let resolved = resolve_credential(&request).unwrap();

    assert_eq!(resolved.api_key, "explicit-key");
    assert_eq!(resolved.provider, "anthropic");
    assert_eq!(resolved.source, CredentialSource::Explicit);
}

#[test]
fn provider_env_vars_are_checked_in_order() {
    let request = CredentialRequest {
        provider: "claude".to_string(),
        env: BTreeMap::from([
            ("ANTHROPIC_TOKEN".to_string(), "fallback-env".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "primary-env".to_string()),
        ]),
        ..CredentialRequest::default()
    };

    let resolved = resolve_credential(&request).unwrap();

    assert_eq!(resolved.provider, "anthropic");
    assert_eq!(resolved.api_key, "primary-env");
    assert_eq!(
        resolved.source,
        CredentialSource::Env {
            variable: "ANTHROPIC_API_KEY".to_string()
        }
    );
}

#[test]
fn pool_resolution_filters_provider_disabled_and_priority() {
    let request = CredentialRequest {
        provider: "openrouter".to_string(),
        explicit_base_url: Some("https://router.example/v1".to_string()),
        credential_pool: vec![
            PooledCredential::new("anthropic", "wrong-provider").priority(-10),
            PooledCredential::new("openrouter", "disabled").enabled(false),
            PooledCredential::new("openrouter", "lower-priority").priority(10),
            PooledCredential::new("openai", "selected")
                .label("primary")
                .priority(0)
                .base_url("https://pool.example/v1"),
        ],
        ..CredentialRequest::default()
    };

    let resolved = resolve_credential(&request).unwrap();

    assert_eq!(resolved.provider, "openrouter");
    assert_eq!(resolved.api_key, "selected");
    assert_eq!(
        resolved.source,
        CredentialSource::Pool {
            label: Some("primary".to_string())
        }
    );
    assert_eq!(
        resolved.base_url.as_deref(),
        Some("https://router.example/v1")
    );
}

#[test]
fn unresolved_error_reports_attempted_sources_without_secret_values() {
    let request = CredentialRequest {
        provider: "gemini".to_string(),
        credential_pool: vec![PooledCredential::new("gemini", "  ")],
        ..CredentialRequest::default()
    };

    let error = resolve_credential(&request).unwrap_err();
    let display = error.to_string();

    assert_eq!(
        error.tried_env_keys,
        vec!["GOOGLE_GENERATIVE_AI_API_KEY", "GEMINI_API_KEY"]
    );
    assert!(display.contains("gemini"));
    assert!(!display.contains("  "));
}

"""Rust/Python parity for setup, provider, model, and auth planning."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CLI_CRATE = REPO_ROOT / "crates" / "hermes-cli"

PROVIDERS = (
    "openrouter",
    "nous",
    "openai-codex",
    "anthropic",
    "lmstudio",
    "copilot-acp",
    "bedrock",
    "gemini",
    "zai",
)


pytestmark = pytest.mark.skipif(
    not RUST_CLI_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-cli setup snapshot not yet built; tracked by hermes-3n2.3",
)


def _rust_snapshot() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-cli",
            "--bin",
            "hermes_cli_setup_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust setup snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot() -> dict:
    from hermes_cli.providers import determine_api_mode, get_provider, normalize_provider
    from hermes_cli.setup import _supports_same_provider_pool_setup

    providers = {}
    for provider in PROVIDERS:
        pdef = get_provider(provider)
        assert pdef is not None, provider
        providers[provider] = {
            "id": pdef.id,
            "name": pdef.name,
            "transport": pdef.transport,
            "api_key_env_vars": list(pdef.api_key_env_vars),
            "base_url": pdef.base_url,
            "base_url_env_var": pdef.base_url_env_var,
            "is_aggregator": pdef.is_aggregator,
            "auth_type": pdef.auth_type,
            "source": pdef.source,
        }

    aliases = {
        alias: normalize_provider(alias)
        for alias in (
            "openai",
            "claude",
            "github-copilot-acp",
            "glm",
            "google",
            "aws-bedrock",
            "lm-studio",
            "ollama",
        )
    }

    api_modes = {
        "openrouter": determine_api_mode("openrouter", ""),
        "anthropic": determine_api_mode("anthropic", ""),
        "openai-codex": determine_api_mode("openai-codex", ""),
        "bedrock": determine_api_mode("bedrock", ""),
        "custom": determine_api_mode("custom", "https://api.openai.com/v1"),
        "custom-anthropic": determine_api_mode("custom", "https://proxy.example/anthropic"),
        "custom-kimi": determine_api_mode("custom", "https://api.kimi.com/coding"),
        "custom-bedrock": determine_api_mode(
            "custom", "https://bedrock-runtime.us-east-1.amazonaws.com"
        ),
    }

    same_provider_pool_support = {
        provider: _supports_same_provider_pool_setup(provider)
        for provider in (
            "openrouter",
            "nous",
            "anthropic",
            "openai-codex",
            "custom",
            "copilot-acp",
            "bedrock",
        )
    }

    return {
        "providers": providers,
        "aliases": aliases,
        "api_modes": api_modes,
        "same_provider_pool_support": same_provider_pool_support,
        "secret_storage": _python_secret_storage(),
        "model_choice_plans": _python_model_choice_plans(),
        "auth_command_choices": _python_auth_command_choices(),
    }


def _python_snapshot_from_subprocess() -> dict:
    code = (
        "import json; "
        "from tests.parity.cli.test_setup import _python_snapshot; "
        "print(json.dumps(_python_snapshot(), sort_keys=True))"
    )
    result = subprocess.run(
        [sys.executable, "-c", code],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Python setup snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_secret_storage() -> dict:
    from hermes_cli.providers import get_provider, normalize_provider

    plans = {}
    for provider in (
        "openrouter",
        "anthropic",
        "nous",
        "openai-codex",
        "custom",
        "copilot-acp",
        "bedrock",
    ):
        normalized = normalize_provider(provider)
        pdef = get_provider(normalized)
        secret_targets = list(pdef.api_key_env_vars) if pdef else []
        if pdef and pdef.auth_type in {"oauth_device_code", "oauth_external"}:
            secret_store = "auth.json"
        elif pdef and pdef.auth_type in {"external_process", "aws_sdk"}:
            secret_store = "external"
        elif secret_targets:
            secret_store = ".env"
        else:
            secret_store = "none"
        plans[provider] = {
            "provider": normalized,
            "secret_store": secret_store,
            "secret_targets": secret_targets,
            "config_keys": [
                "model.provider",
                "model.default",
                "model.base_url",
                "model.api_mode",
            ],
        }
    return plans


def _apply_model_choice(config, provider, model_name, base_url=None, api_mode=None) -> dict:
    from copy import deepcopy
    from hermes_cli.providers import normalize_provider

    config = deepcopy(config)
    if not isinstance(config, dict):
        config = {}
    model = config.get("model")
    if isinstance(model, dict):
        model = dict(model)
    elif isinstance(model, str) and model.strip():
        model = {"default": model}
    else:
        model = {}
    provider = normalize_provider(provider)
    model["provider"] = provider
    if model_name.strip():
        model["default"] = model_name.strip()
    if base_url and base_url.strip():
        model["base_url"] = base_url.strip()
    else:
        model.pop("base_url", None)
    if api_mode and api_mode.strip():
        model["api_mode"] = api_mode.strip()
    else:
        model.pop("api_mode", None)
    config["model"] = model
    return {
        "provider": provider,
        "model": model_name,
        "config": config,
        "config_keys_written": [
            "model.provider",
            "model.default",
            "model.base_url",
            "model.api_mode",
        ],
        "secret_keys_written_to_config": [],
    }


def _python_model_choice_plans() -> dict:
    base = {
        "terminal": {"timeout": 999},
        "display": {"skin": "mono"},
        "model": {
            "default": "old-model",
            "provider": "custom",
            "base_url": "http://localhost:11434/v1",
            "api_mode": "chat_completions",
        },
    }
    return {
        "switch_custom_to_codex": _apply_model_choice(
            base,
            "openai-codex",
            "gpt-5.3-codex",
            "https://api.openai.com/v1",
            "codex_responses",
        ),
        "switch_to_openrouter_preserves_other_config": _apply_model_choice(
            base,
            "openrouter",
            "anthropic/claude-opus-4.6",
            "https://openrouter.ai/api/v1",
            None,
        ),
        "string_model_becomes_dict": _apply_model_choice(
            {"model": "legacy-model", "terminal": {"timeout": 50}},
            "zai",
            "glm-5",
            "https://api.z.ai/api/paas/v4",
            None,
        ),
    }


def _python_auth_command_choices() -> dict:
    return {
        "login_providers": ["nous", "openai-codex"],
        "logout_providers": ["nous", "openai-codex", "spotify"],
        "auth_subcommands": ["add", "list", "remove", "reset", "status", "logout", "spotify"],
    }


def test_rust_setup_provider_and_auth_snapshot_matches_python_helpers() -> None:
    assert _rust_snapshot() == _python_snapshot_from_subprocess()


def test_rust_setup_contract_keeps_secrets_out_of_config() -> None:
    snapshot = _rust_snapshot()
    assert snapshot["secret_storage"]["openrouter"]["secret_store"] == ".env"
    assert snapshot["secret_storage"]["nous"]["secret_store"] == "auth.json"
    assert snapshot["secret_storage"]["copilot-acp"]["secret_store"] == "external"
    for plan in snapshot["model_choice_plans"].values():
        assert plan["secret_keys_written_to_config"] == []
        assert "terminal" in plan["config"]

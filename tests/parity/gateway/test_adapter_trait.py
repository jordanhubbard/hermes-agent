"""Rust/Python parity for the gateway platform-adapter boundary."""

from __future__ import annotations

import dataclasses
import inspect
import json
import shutil
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_GATEWAY_CRATE = REPO_ROOT / "crates" / "hermes-gateway"


pytestmark = pytest.mark.skipif(
    not RUST_GATEWAY_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-gateway not yet built; tracked by hermes-4ne.4",
)


def _rust_snapshot() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-gateway",
            "--bin",
            "hermes_gateway_adapter_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust gateway adapter snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot() -> dict:
    from gateway.config import _BUILTIN_PLATFORM_VALUES
    from gateway.platform_registry import PlatformEntry
    from gateway.platforms.base import BasePlatformAdapter

    abstract_methods = sorted(BasePlatformAdapter.__abstractmethods__)
    entry_fields = [field.name for field in dataclasses.fields(PlatformEntry)]
    return {
        "abstract_methods": abstract_methods,
        "platform_entry_fields": entry_fields,
        "builtin_platform_values": sorted(_BUILTIN_PLATFORM_VALUES),
        "send_signature": str(inspect.signature(BasePlatformAdapter.send)),
    }


def test_rust_adapter_boundary_covers_python_base_contract() -> None:
    rust = _rust_snapshot()
    python = _python_snapshot()

    for method in python["abstract_methods"]:
        assert method in rust["adapter_trait_methods"]
    for required in ("start", "stop", "receive", "status", "acquire_token_lock", "release_token_lock"):
        assert required in rust["adapter_trait_methods"]

    for field in (
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
    ):
        assert field in python["platform_entry_fields"]
        assert field in rust["platform_entry_fields"]

    assert sorted(rust["builtin_platform_values"]) == python["builtin_platform_values"]


def test_rust_low_risk_adapter_smoke_runs_end_to_end() -> None:
    status = _rust_snapshot()["smoke_status"]
    assert status == {
        "platform": "webhook",
        "label": "Webhook",
        "connected": True,
        "started": True,
        "sent_count": 1,
        "pending_count": 0,
        "token_lock_scope": "webhook_token",
    }

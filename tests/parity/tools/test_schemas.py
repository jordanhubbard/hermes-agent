"""Rust/Python parity for tool schema discovery and toolset resolution."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_TOOLS_CRATE = REPO_ROOT / "crates" / "hermes-tools"

RESOLVED_TOOLSET_SAMPLES = (
    "web",
    "file",
    "browser",
    "debugging",
    "hermes-cli",
    "hermes-discord",
    "hermes-feishu",
    "hermes-gateway",
    "terminal_tools",
    "all",
    "unknown",
)

SCHEMA_CASES = (
    ("default", None, None),
    ("hermes-cli", ["hermes-cli"], None),
    ("file", ["file"], None),
    ("browser", ["browser"], None),
    ("debugging_no_file", ["debugging"], ["file"]),
    ("discord_without_browser", ["hermes-discord"], ["browser"]),
    ("legacy_file_tools", ["file_tools"], None),
)


pytestmark = pytest.mark.skipif(
    not RUST_TOOLS_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-tools not yet built; tracked by hermes-k77.1",
)


def _rust_snapshot() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-tools",
            "--bin",
            "hermes_tools_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust tools snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot() -> dict:
    import model_tools
    import toolsets

    resolved = {}
    for name in RESOLVED_TOOLSET_SAMPLES:
        if name in model_tools._LEGACY_TOOLSET_MAP:
            resolved[name] = sorted(model_tools._LEGACY_TOOLSET_MAP[name])
        elif toolsets.validate_toolset(name):
            resolved[name] = toolsets.resolve_toolset(name)
        else:
            resolved[name] = []

    schema_names = {}
    schemas = {}
    for label, enabled, disabled in SCHEMA_CASES:
        selected = model_tools.get_tool_definitions(
            enabled_toolsets=enabled,
            disabled_toolsets=disabled,
            quiet_mode=True,
        )
        schema_names[label] = [schema["function"]["name"] for schema in selected]
        schemas[label] = _normalize_schema_paths(selected)

    default_before = model_tools.get_tool_definitions(quiet_mode=True)
    file_only = model_tools.get_tool_definitions(enabled_toolsets=["file"], quiet_mode=True)
    default_after = model_tools.get_tool_definitions(quiet_mode=True)

    return {
        "core_tools": list(toolsets._HERMES_CORE_TOOLS),
        "all_toolsets": sorted(toolsets.get_all_toolsets().keys()),
        "registered_tool_names": [
            schema["function"]["name"]
            for schema in model_tools.get_tool_definitions(quiet_mode=True)
        ],
        "resolved_toolsets": resolved,
        "schema_names": schema_names,
        "schemas": schemas,
        "cache_isolation": {
            "default_names_before": [schema["function"]["name"] for schema in default_before],
            "file_names": [schema["function"]["name"] for schema in file_only],
            "default_names_after": [schema["function"]["name"] for schema in default_after],
            "cache_keys_distinct": (
                [schema["function"]["name"] for schema in default_before]
                != [schema["function"]["name"] for schema in file_only]
            ),
        },
    }


def _python_snapshot_from_subprocess() -> dict:
    code = (
        "import json; "
        "from tests.parity.tools.test_schemas import _python_snapshot; "
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
        f"Python tools snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _normalize_schema_paths(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: _normalize_schema_paths(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_normalize_schema_paths(item) for item in value]
    if isinstance(value, str):
        value = re.sub(r"New skills go to .*?/skills/", "New skills go to <HERMES_HOME>/skills/", value)
        value = value.replace("New skills go to ~/.hermes/skills/", "New skills go to <HERMES_HOME>/skills/")
        value = re.sub(r"Defaults to .*?/audio_cache/", "Defaults to <HERMES_HOME>/audio_cache/", value)
        value = value.replace("Defaults to ~/.hermes/audio_cache/", "Defaults to <HERMES_HOME>/audio_cache/")
    return value


def _normalized_rust_snapshot() -> dict:
    snapshot = _rust_snapshot()
    snapshot["schemas"] = _normalize_schema_paths(snapshot["schemas"])
    snapshot.pop("tool_to_toolset", None)
    return snapshot


def test_rust_tool_schema_and_toolset_snapshot_matches_python() -> None:
    assert _normalized_rust_snapshot() == _python_snapshot_from_subprocess()


def test_rust_tool_resolution_covers_cache_and_legacy_edges() -> None:
    snapshot = _normalized_rust_snapshot()
    assert snapshot["resolved_toolsets"]["terminal_tools"] == ["terminal"]
    assert snapshot["resolved_toolsets"]["unknown"] == []
    assert snapshot["cache_isolation"]["default_names_before"] == snapshot["cache_isolation"]["default_names_after"]
    assert snapshot["cache_isolation"]["cache_keys_distinct"] is True
    assert all(
        not name.startswith("browser_")
        for name in snapshot["schema_names"]["discord_without_browser"]
    )

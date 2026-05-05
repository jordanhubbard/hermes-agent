"""Rust/Python parity for the ACP adapter session, event, permission, and tool boundary."""

from __future__ import annotations

import dataclasses
import inspect
import json
import re
import shutil
import subprocess
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_ACP_CRATE = REPO_ROOT / "crates" / "hermes-acp"
ACP_SERVER = REPO_ROOT / "acp_adapter" / "server.py"

pytestmark = pytest.mark.skipif(
    not RUST_ACP_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-acp not yet built; tracked by hermes-dwg.2",
)


def _rust_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-acp",
            "--bin",
            "hermes_acp_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust ACP snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_server_methods() -> set[str]:
    source = ACP_SERVER.read_text(encoding="utf-8")
    body = source.split("class HermesACPAgent", 1)[1]
    return {
        name
        for name in re.findall(r"^    async def ([A-Za-z_]\w*)\(", body, re.M)
        if not name.startswith("_")
    }


def test_acp_server_method_and_capability_contract_matches_python() -> None:
    rust = _rust_snapshot()
    rust_methods = {method["name"] for method in rust["server_methods"]}

    assert rust_methods == _python_server_methods()

    from acp_adapter.server import HermesACPAgent
    from acp_adapter.session import SessionManager

    agent = HermesACPAgent(
        session_manager=SessionManager(
            agent_factory=lambda: SimpleNamespace(model="gpt-5.4", provider="openai")
        )
    )

    async def _initialize():
        return await agent.initialize(protocol_version=1)

    import asyncio

    response = asyncio.run(_initialize())
    caps = response.agent_capabilities.model_dump(by_alias=True, exclude_none=True)

    assert rust["capabilities"] == {
        "load_session": caps["loadSession"],
        "prompt_image": caps["promptCapabilities"]["image"],
        "session_fork": "fork" in caps["sessionCapabilities"],
        "session_list": "list" in caps["sessionCapabilities"],
        "session_resume": "resume" in caps["sessionCapabilities"],
    }


def test_advertised_commands_match_python_available_commands() -> None:
    from acp_adapter.server import HermesACPAgent

    rust_commands = _rust_snapshot()["advertised_commands"]
    python_commands = [
        {
            "name": command.name,
            "description": command.description,
            "input_hint": command.input.root.hint if command.input is not None else None,
        }
        for command in HermesACPAgent._available_commands()
    ]

    assert rust_commands == python_commands


def test_session_manager_state_and_persistence_contract_matches_python() -> None:
    from acp_adapter.session import (
        SessionManager,
        SessionState,
        _expand_acp_enabled_toolsets,
        _normalize_cwd_for_compare,
    )

    rust_session = _rust_snapshot()["session"]

    assert rust_session["state_fields"] == [field.name for field in dataclasses.fields(SessionState)]

    public_methods = {
        name
        for name, fn in inspect.getmembers(SessionManager, inspect.isfunction)
        if not name.startswith("_")
    }
    assert set(rust_session["manager_methods"]) == public_methods
    assert rust_session["source"] == "acp"
    assert set(rust_session["model_config_keys"]) == {"cwd", "provider", "base_url", "api_mode"}
    assert rust_session["list_page_size"] == 50
    assert rust_session["expanded_toolsets"] == _expand_acp_enabled_toolsets(
        ["hermes-acp"],
        ["filesystem", "github"],
    )
    for raw, normalized in rust_session["cwd_normalization_samples"].items():
        assert _normalize_cwd_for_compare(raw) == normalized


def test_permission_mapping_matches_python_approval_bridge() -> None:
    from acp_adapter.permissions import _KIND_TO_HERMES

    rust = _rust_snapshot()
    mapping_cases = {
        case["kind"]: case["hermes_result"]
        for case in rust["permission_cases"]
        if case["kind"] in _KIND_TO_HERMES
    }

    assert mapping_cases == _KIND_TO_HERMES
    denied = {case["option_id"]: case["hermes_result"] for case in rust["permission_cases"]}
    assert denied["timeout"] == "deny"
    assert denied["none_response"] == "deny"


def test_tool_kind_map_and_title_samples_match_python_tools() -> None:
    from acp_adapter.tools import TOOL_KIND_MAP, _POLISHED_TOOLS, build_tool_title, get_tool_kind

    rust = _rust_snapshot()

    assert rust["tool_kind_map"] == {name: str(kind) for name, kind in TOOL_KIND_MAP.items()}
    assert set(rust["polished_tools"]) == set(_POLISHED_TOOLS)

    for sample in rust["tool_title_samples"]:
        assert get_tool_kind(sample["tool"]) == sample["kind"]
        assert build_tool_title(sample["tool"], sample["args"]) == sample["title"]


def test_event_and_tool_rendering_contract_covers_acp_updates() -> None:
    rust = _rust_snapshot()
    callbacks = {item["callback"]: item for item in rust["event_callbacks"]}

    assert callbacks["tool_progress_callback"]["input"] == "tool.started"
    assert callbacks["tool_progress_callback"]["update"] == "ToolCallStart"
    assert "fifo_id_by_tool_name" in callbacks["tool_progress_callback"]["tracking"]
    assert callbacks["step_callback"]["update"] == "ToolCallProgress"
    assert callbacks["thinking_callback"]["update"] == "AgentThoughtChunk"
    assert callbacks["message_callback"]["update"] == "AgentMessageChunk"
    assert callbacks["usage_update"]["update"] == "UsageUpdate"
    assert callbacks["available_commands"]["update"] == "AvailableCommandsUpdate"

    rendering = {(item["tool"], item["phase"]): item for item in rust["tool_rendering"]}
    assert rendering[("read_file", "start")]["content"] == "compact_no_content"
    assert rendering[("web_extract", "complete")]["content"] == "compact_on_success_error_text_on_failure"
    assert rendering[("patch", "complete")]["content"] == "structured_diff_or_text_fallback"
    assert rendering[("unknown", "complete")]["raw_passthrough"] is True

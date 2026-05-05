"""Rust/Python/Ink parity for the TUI gateway JSON-RPC protocol."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_TUI_CRATE = REPO_ROOT / "crates" / "hermes-tui-gateway"
SERVER_PY = REPO_ROOT / "tui_gateway" / "server.py"
ENTRY_PY = REPO_ROOT / "tui_gateway" / "entry.py"
WS_PY = REPO_ROOT / "tui_gateway" / "ws.py"
UI_SRC = REPO_ROOT / "ui-tui" / "src"
GATEWAY_TYPES_TS = UI_SRC / "gatewayTypes.ts"

pytestmark = pytest.mark.skipif(
    not RUST_TUI_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-tui-gateway not yet built; tracked by hermes-dwg.1",
)


def _rust_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-tui-gateway",
            "--bin",
            "hermes_tui_gateway_protocol_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust TUI gateway snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_method_names() -> set[str]:
    return set(re.findall(r'@method\("([^"]+)"\)', SERVER_PY.read_text(encoding="utf-8")))


def _python_long_handlers() -> set[str]:
    source = SERVER_PY.read_text(encoding="utf-8")
    match = re.search(r"_LONG_HANDLERS\s*=\s*frozenset\(\s*\{(?P<body>.*?)\}\s*\)", source, re.S)
    assert match, "_LONG_HANDLERS not found in tui_gateway/server.py"
    return set(re.findall(r'"([^"]+)"', match.group("body")))


def _ui_rpc_methods() -> set[str]:
    methods: set[str] = set()
    pattern = re.compile(r"(?:request|rpc|respondWith)(?:<[^>]+>)?\(\s*['\"]([a-z_]+\.[a-z_]+)['\"]")
    for path in UI_SRC.rglob("*.ts*"):
        if "__tests__" in path.parts:
            continue
        methods.update(pattern.findall(path.read_text(encoding="utf-8")))
    return methods


def _ui_gateway_events() -> set[str]:
    source = GATEWAY_TYPES_TS.read_text(encoding="utf-8")
    block = source.split("export type GatewayEvent =", 1)[1]
    return {value for value in re.findall(r"'([^']+)'", block) if "." in value or value == "error"}


def _python_static_events() -> set[str]:
    events: set[str] = set()
    server = SERVER_PY.read_text(encoding="utf-8")
    for fn in ("_emit", "_block", "_voice_emit"):
        events.update(re.findall(rf"{fn}\(\s*['\"]([^'\"]+)['\"]", server))
    for path in (ENTRY_PY, WS_PY):
        events.update(re.findall(r'"type"\s*:\s*"([^"]+)"', path.read_text(encoding="utf-8")))
    return events


def test_rust_method_catalog_matches_python_server_and_covers_ink_requests() -> None:
    rust = _rust_snapshot()
    rust_methods = {method["name"] for method in rust["methods"]}
    python_methods = _python_method_names()
    ui_methods = _ui_rpc_methods()

    assert rust_methods == python_methods
    assert ui_methods <= rust_methods
    assert set(rust["required_acceptance_methods"]) <= rust_methods

    long_from_rust = {method["name"] for method in rust["methods"] if method["long_handler"]}
    assert long_from_rust == _python_long_handlers()
    assert set(rust["long_handlers"]) == _python_long_handlers()


def test_core_acceptance_methods_have_frontend_visible_contracts() -> None:
    by_name = {method["name"]: method for method in _rust_snapshot()["methods"]}

    prompt = by_name["prompt.submit"]
    assert {"session_id", "text"} <= set(prompt["params"])
    assert "status" in prompt["result_fields"]
    assert {"message.start", "message.delta", "message.complete", "error"} <= set(prompt["emits"])

    slash = by_name["slash.exec"]
    assert {"session_id", "command"} <= set(slash["params"])
    assert {"output", "warning"} <= set(slash["result_fields"])
    assert slash["long_handler"] is True

    approval = by_name["approval.respond"]
    assert {"session_id", "choice", "all"} <= set(approval["params"])
    assert approval["result_fields"] == ["resolved"]

    for name in ("complete.path", "complete.slash"):
        assert "items" in by_name[name]["result_fields"]

    assert "sessions" in by_name["session.list"]["result_fields"]
    assert {"session_id", "messages", "message_count", "info"} <= set(
        by_name["session.resume"]["result_fields"]
    )


def test_rust_event_catalog_matches_ink_gateway_event_union_and_python_emitters() -> None:
    rust_events = {event["event_type"] for event in _rust_snapshot()["events"]}
    ui_events = _ui_gateway_events()
    python_events = _python_static_events()

    assert rust_events == ui_events
    assert python_events <= rust_events


def test_event_stream_sequences_cover_prompt_tools_and_blocking_prompts() -> None:
    sequences = {seq["name"]: seq["events"] for seq in _rust_snapshot()["stream_sequences"]}

    assert sequences["prompt.submit.success"] == [
        "message.start",
        "message.delta",
        "message.complete",
    ]
    assert sequences["tool.call.visible"] == [
        "tool.start",
        "tool.progress",
        "tool.complete",
    ]
    assert sequences["approval.prompt"] == ["approval.request"]
    assert sequences["blocking.prompts"] == [
        "clarify.request",
        "sudo.request",
        "secret.request",
    ]


def test_json_rpc_error_envelopes_match_python_protocol_tests() -> None:
    cases = {case["name"]: case["response"] for case in _rust_snapshot()["json_rpc_cases"]}

    assert cases["non_object"] == {
        "jsonrpc": "2.0",
        "id": None,
        "error": {"code": -32600, "message": "invalid request: expected an object"},
    }
    assert cases["missing_method"] == {
        "jsonrpc": "2.0",
        "id": "1",
        "error": {
            "code": -32600,
            "message": "invalid request: method must be a non-empty string",
        },
    }
    assert cases["array_params"] == {
        "jsonrpc": "2.0",
        "id": "5",
        "error": {"code": -32602, "message": "invalid params: expected an object"},
    }
    assert cases["unknown_method"] == {
        "jsonrpc": "2.0",
        "id": "6",
        "error": {"code": -32601, "message": "unknown method: bogus"},
    }

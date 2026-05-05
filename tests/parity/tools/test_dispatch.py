"""Rust/Python parity for tool dispatch envelopes and hook ordering."""

from __future__ import annotations

import copy
import json
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_TOOLS_CRATE = REPO_ROOT / "crates" / "hermes-tools"

pytestmark = pytest.mark.skipif(
    not RUST_TOOLS_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-tools not yet built; tracked by hermes-k77.2",
)


def _rust_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-tools",
            "--bin",
            "hermes_tools_dispatch_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust tools dispatch snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot(monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    return {
        "registry_dispatch": _python_registry_dispatch_snapshot(),
        "handle_function_call": _python_handle_function_call_snapshot(monkeypatch),
    }


def _python_registry_dispatch_snapshot() -> dict[str, Any]:
    from tools.registry import ToolRegistry

    registry = ToolRegistry()

    def success_handler(args: dict[str, Any], **kwargs: Any) -> str:
        return _handler_result(
            "typed_tool",
            args,
            task_id=kwargs.get("task_id"),
            user_task=kwargs.get("user_task"),
            enabled_tools=kwargs.get("enabled_tools"),
        )

    def failing_handler(_args: dict[str, Any], **_kwargs: Any) -> str:
        raise RuntimeError("boom")

    registry.register(
        "typed_tool",
        "test",
        _typed_tool_schema(),
        success_handler,
    )
    registry.register(
        "failing_tool",
        "test",
        {"name": "failing_tool", "parameters": {"type": "object", "properties": {}}},
        failing_handler,
    )

    cases = {
        "success": registry.dispatch("typed_tool", {"value": "ok"}),
        "unknown_tool": registry.dispatch("missing_tool", {"value": "ok"}),
        "handler_exception": registry.dispatch("failing_tool", {"value": "ok"}),
    }
    return {
        label: {"result": result, "parsed_result": _parse_json(result)}
        for label, result in cases.items()
    }


def _python_handle_function_call_snapshot(monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    import model_tools

    snapshots: dict[str, Any] = {}
    for case in _dispatch_cases():
        dispatches: list[dict[str, Any]] = []
        hook_events: list[dict[str, Any]] = []
        notifications: list[str] = []

        args = copy.deepcopy(case["args"])

        def fake_get_schema(tool_name: str) -> dict[str, Any] | None:
            if tool_name == case["tool_name"]:
                return copy.deepcopy(case.get("schema"))
            return None

        def fake_dispatch(name: str, call_args: dict[str, Any], **kwargs: Any) -> str:
            enabled_tools = kwargs.get("enabled_tools") if name == "execute_code" else None
            user_task = None if name == "execute_code" else kwargs.get("user_task")
            dispatches.append(
                {
                    "tool_name": name,
                    "args": copy.deepcopy(call_args),
                    "task_id": kwargs.get("task_id"),
                    "user_task": user_task,
                    "enabled_tools": copy.deepcopy(enabled_tools),
                }
            )
            mode = case["handler_mode"]
            if mode == "unknown":
                return json.dumps({"error": f"Unknown tool: {name}"})
            if mode == "registry_failure":
                return json.dumps({"error": "Tool execution failed: RuntimeError: boom"})
            if mode == "outer_exception":
                raise RuntimeError("outer boom")
            return _handler_result(
                name,
                call_args,
                task_id=kwargs.get("task_id"),
                user_task=user_task,
                enabled_tools=enabled_tools,
            )

        def fake_invoke_hook(hook_name: str, **kwargs: Any) -> list[Any]:
            event = {
                "hook": hook_name,
                "tool_name": kwargs["tool_name"],
                "args": copy.deepcopy(kwargs.get("args") if isinstance(kwargs.get("args"), dict) else {}),
                "result": kwargs.get("result"),
                "task_id": kwargs.get("task_id", ""),
                "session_id": kwargs.get("session_id", ""),
                "tool_call_id": kwargs.get("tool_call_id", ""),
                "duration_ms": None,
            }
            if "duration_ms" in kwargs:
                assert isinstance(kwargs["duration_ms"], int)
                assert kwargs["duration_ms"] >= 0
                event["duration_ms"] = "non_negative_int"
            hook_events.append(event)

            plan = case["hooks"].get(hook_name, [])
            if plan == "raise":
                raise RuntimeError("hook boom")
            return copy.deepcopy(plan)

        def fake_notify(task_id: str) -> None:
            notifications.append(task_id)

        with monkeypatch.context() as mp:
            mp.setattr(model_tools.registry, "get_schema", fake_get_schema)
            mp.setattr(model_tools.registry, "dispatch", fake_dispatch)
            mp.setattr("hermes_cli.plugins.invoke_hook", fake_invoke_hook)
            mp.setattr("tools.file_tools.notify_other_tool_call", fake_notify)

            result = model_tools.handle_function_call(
                case["tool_name"],
                args,
                task_id=case.get("task_id"),
                session_id=case.get("session_id"),
                tool_call_id=case.get("tool_call_id"),
                user_task=case.get("user_task"),
                enabled_tools=case.get("enabled_tools"),
                skip_pre_tool_call_hook=case.get("skip_pre_tool_call_hook", False),
            )

        snapshots[case["label"]] = {
            "args_after_coercion": args,
            "dispatches": dispatches,
            "hook_events": hook_events,
            "notifications": notifications,
            "parsed_result": _parse_json(result),
            "result": result,
        }

    return snapshots


def _dispatch_cases() -> list[dict[str, Any]]:
    typed_schema = _typed_tool_schema()
    return [
        {
            "label": "coerce_success",
            "tool_name": "typed_tool",
            "args": {
                "config": '{"max":50}',
                "extra": "42",
                "full": "false",
                "limit": "10",
                "nullable": "null",
                "path": "readme.md",
                "temperature": "0.7",
                "urls": "https://a.com",
            },
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-1",
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": _empty_hooks(),
            "schema": typed_schema,
        },
        {
            "label": "unknown_tool",
            "tool_name": "missing_tool",
            "args": {},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-unknown",
            "user_task": "user goal",
            "handler_mode": "unknown",
            "hooks": _empty_hooks(),
        },
        {
            "label": "handler_exception",
            "tool_name": "failing_tool",
            "args": {"value": "x"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-fail",
            "user_task": "user goal",
            "handler_mode": "registry_failure",
            "hooks": _empty_hooks(),
        },
        {
            "label": "outer_dispatch_exception",
            "tool_name": "exploding_dispatch",
            "args": {"value": "x"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-outer",
            "user_task": "user goal",
            "handler_mode": "outer_exception",
            "hooks": _empty_hooks(),
        },
        {
            "label": "agent_loop_tool",
            "tool_name": "todo",
            "args": {"action": "list"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-agent-loop",
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": _empty_hooks(),
        },
        {
            "label": "pre_hook_block",
            "tool_name": "web_search",
            "args": {"q": "test"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-block",
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": {
                "pre_tool_call": [{"action": "block", "message": "Blocked by policy"}],
                "post_tool_call": [],
                "transform_tool_result": [],
            },
        },
        {
            "label": "invalid_pre_hook_returns",
            "tool_name": "web_search",
            "args": {"q": "test"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-invalid-pre",
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": {
                "pre_tool_call": [
                    "block",
                    {"action": "block"},
                    {"action": "deny", "message": "nope"},
                ],
                "post_tool_call": [],
                "transform_tool_result": [],
            },
        },
        {
            "label": "skip_pre_hook",
            "tool_name": "web_search",
            "args": {"q": "test"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-skip",
            "user_task": "user goal",
            "handler_mode": "success",
            "skip_pre_tool_call_hook": True,
            "hooks": {
                "pre_tool_call": [{"action": "block", "message": "should not fire"}],
                "post_tool_call": [],
                "transform_tool_result": [],
            },
        },
        {
            "label": "post_observational",
            "tool_name": "typed_tool",
            "args": {"limit": "3"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-post",
            "user_task": "user goal",
            "handler_mode": "success",
            "skip_pre_tool_call_hook": True,
            "hooks": {
                "pre_tool_call": [],
                "post_tool_call": ["observer return should be ignored"],
                "transform_tool_result": [],
            },
            "schema": typed_schema,
        },
        {
            "label": "transform_first_string",
            "tool_name": "typed_tool",
            "args": {"limit": "4"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-transform",
            "user_task": "user goal",
            "handler_mode": "success",
            "skip_pre_tool_call_hook": True,
            "hooks": {
                "pre_tool_call": [],
                "post_tool_call": [],
                "transform_tool_result": [None, {"bad": True}, "rewritten", "second"],
            },
            "schema": typed_schema,
        },
        {
            "label": "non_string_transform_ignored",
            "tool_name": "typed_tool",
            "args": {"limit": "5"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-transform-ignore",
            "user_task": "user goal",
            "handler_mode": "success",
            "skip_pre_tool_call_hook": True,
            "hooks": {
                "pre_tool_call": [],
                "post_tool_call": [],
                "transform_tool_result": [{"bad": True}, 123, ["nope"]],
            },
            "schema": typed_schema,
        },
        {
            "label": "transform_hook_exception",
            "tool_name": "typed_tool",
            "args": {"limit": "6"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-transform-exception",
            "user_task": "user goal",
            "handler_mode": "success",
            "skip_pre_tool_call_hook": True,
            "hooks": {
                "pre_tool_call": [],
                "post_tool_call": [],
                "transform_tool_result": "raise",
            },
            "schema": typed_schema,
        },
        {
            "label": "execute_code_enabled_tools",
            "tool_name": "execute_code",
            "args": {"code": "print(1)"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-code",
            "user_task": "user goal",
            "enabled_tools": ["terminal", "read_file"],
            "handler_mode": "success",
            "hooks": _empty_hooks(),
        },
        {
            "label": "read_file_no_notify",
            "tool_name": "read_file",
            "args": {"limit": "100", "offset": "10", "path": "foo.py"},
            "task_id": "task-1",
            "session_id": "session-1",
            "tool_call_id": "call-read",
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": _empty_hooks(),
            "schema": _read_file_schema(),
        },
        {
            "label": "terminal_default_task_notification",
            "tool_name": "terminal",
            "args": {"command": "pwd"},
            "task_id": None,
            "session_id": None,
            "tool_call_id": None,
            "user_task": "user goal",
            "handler_mode": "success",
            "hooks": _empty_hooks(),
        },
    ]


def _empty_hooks() -> dict[str, list[Any]]:
    return {"pre_tool_call": [], "post_tool_call": [], "transform_tool_result": []}


def _typed_tool_schema() -> dict[str, Any]:
    return {
        "name": "typed_tool",
        "description": "test",
        "parameters": {
            "type": "object",
            "properties": {
                "config": {"type": "object"},
                "full": {"type": "boolean"},
                "limit": {"type": "integer"},
                "nullable": {"type": "object", "nullable": True, "default": None},
                "path": {"type": "string"},
                "temperature": {"type": "number"},
                "urls": {"type": "array", "items": {"type": "string"}},
            },
        },
    }


def _read_file_schema() -> dict[str, Any]:
    return {
        "name": "read_file",
        "description": "test",
        "parameters": {
            "type": "object",
            "properties": {
                "limit": {"type": "integer"},
                "offset": {"type": "integer"},
                "path": {"type": "string"},
            },
        },
    }


def _handler_result(
    tool_name: str,
    args: dict[str, Any],
    *,
    task_id: str | None,
    user_task: str | None,
    enabled_tools: list[str] | None,
) -> str:
    return json.dumps(
        {
            "args": args,
            "enabled_tools": enabled_tools,
            "task_id": task_id,
            "tool_name": tool_name,
            "user_task": user_task,
        },
        sort_keys=True,
        separators=(",", ":"),
    )


def _parse_json(result: str) -> Any:
    try:
        return json.loads(result)
    except ValueError:
        return None


def test_rust_dispatch_snapshot_matches_python(monkeypatch: pytest.MonkeyPatch) -> None:
    assert _rust_snapshot() == _python_snapshot(monkeypatch)


def test_dispatch_snapshot_covers_contract_edges(monkeypatch: pytest.MonkeyPatch) -> None:
    snapshot = _python_snapshot(monkeypatch)
    cases = snapshot["handle_function_call"]

    assert cases["agent_loop_tool"]["dispatches"] == []
    assert cases["pre_hook_block"]["notifications"] == []
    assert cases["read_file_no_notify"]["notifications"] == []
    assert cases["terminal_default_task_notification"]["notifications"] == ["default"]
    assert cases["skip_pre_hook"]["hook_events"][0]["hook"] == "post_tool_call"
    assert cases["transform_first_string"]["result"] == "rewritten"
    assert cases["post_observational"]["parsed_result"]["args"] == {"limit": 3}
    assert cases["execute_code_enabled_tools"]["dispatches"][0]["enabled_tools"] == [
        "terminal",
        "read_file",
    ]

"""Fixture schema, validation, and reference replay for parity tests.

The reference replay simulates a minimal agent loop driven entirely by the
fixture's canned model responses. It does not call run_agent.py — it defines
the *contract* that any backend (Python or Rust) must satisfy when replaying
the same fixture.

A real backend integration test plugs the same fixture into AIAgent (or the
Rust agent-core) by injecting the canned model responses as the provider
client and the canned tool results as the tool dispatcher. That integration
work depends on hermes-1oa landing.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

FIXTURE_DIR = Path(__file__).parent / "fixtures"

REQUIRED_TOP_LEVEL = ("id", "description", "inputs", "expected")
REQUIRED_INPUTS = ("user_messages", "tool_definitions", "canned_model_responses")
REQUIRED_EXPECTED = (
    "turn_count",
    "tool_calls_dispatched",
    "tool_results_persisted",
    "final_message",
    "persisted_message_count",
    "persisted_roles",
    "reasoning_fields_present",
    "errors",
)


class FixtureError(Exception):
    """Raised when a fixture violates the schema."""


@dataclass
class ReplayResult:
    """Captured behavior from one replay of a fixture."""

    turn_count: int = 0
    tool_calls_dispatched: list[dict[str, Any]] = field(default_factory=list)
    tool_results_persisted: list[dict[str, Any]] = field(default_factory=list)
    final_message: dict[str, Any] | None = None
    persisted_messages: list[dict[str, Any]] = field(default_factory=list)
    reasoning_fields_present: bool = False
    errors: list[str] = field(default_factory=list)


def load_fixture(path: Path) -> dict[str, Any]:
    with path.open() as fh:
        data = json.load(fh)
    validate_fixture(data, source=str(path))
    return data


def iter_fixtures(fixture_dir: Path = FIXTURE_DIR) -> Iterable[tuple[Path, dict[str, Any]]]:
    for path in sorted(fixture_dir.glob("*.json")):
        yield path, load_fixture(path)


def validate_fixture(data: dict[str, Any], *, source: str = "<fixture>") -> None:
    """Check structural invariants. Raises FixtureError on violation."""
    for key in REQUIRED_TOP_LEVEL:
        if key not in data:
            raise FixtureError(f"{source}: missing top-level key '{key}'")
    inputs = data["inputs"]
    for key in REQUIRED_INPUTS:
        if key not in inputs:
            raise FixtureError(f"{source}: inputs missing '{key}'")
    if not isinstance(inputs["user_messages"], list) or not inputs["user_messages"]:
        raise FixtureError(f"{source}: user_messages must be a non-empty list")
    if not isinstance(inputs["tool_definitions"], list):
        raise FixtureError(f"{source}: tool_definitions must be a list")
    if not isinstance(inputs["canned_model_responses"], list) or not inputs["canned_model_responses"]:
        raise FixtureError(f"{source}: canned_model_responses must be non-empty")

    canned_results = inputs.get("canned_tool_results") or {}
    if not isinstance(canned_results, dict):
        raise FixtureError(f"{source}: canned_tool_results must be an object")

    referenced_call_ids: set[str] = set()
    for resp in inputs["canned_model_responses"]:
        if not isinstance(resp, dict):
            raise FixtureError(f"{source}: each canned response must be an object")
        if "tool_calls" in resp:
            if not isinstance(resp["tool_calls"], list):
                raise FixtureError(f"{source}: tool_calls must be a list")
            for tc in resp["tool_calls"]:
                if not isinstance(tc, dict):
                    raise FixtureError(f"{source}: tool_call must be an object")
                for k in ("id", "name", "arguments"):
                    if k not in tc:
                        raise FixtureError(f"{source}: tool_call missing '{k}'")
                if not isinstance(tc["arguments"], dict):
                    raise FixtureError(
                        f"{source}: tool_call.arguments must be an object"
                    )
                referenced_call_ids.add(tc["id"])
    missing = referenced_call_ids - canned_results.keys()
    if missing:
        raise FixtureError(
            f"{source}: tool_calls reference call_ids with no canned result: {sorted(missing)}"
        )

    expected = data["expected"]
    for key in REQUIRED_EXPECTED:
        if key not in expected:
            raise FixtureError(f"{source}: expected missing '{key}'")
    if not isinstance(expected["persisted_roles"], list):
        raise FixtureError(f"{source}: expected.persisted_roles must be a list")
    if expected["persisted_message_count"] != len(expected["persisted_roles"]):
        raise FixtureError(
            f"{source}: persisted_message_count ({expected['persisted_message_count']}) "
            f"does not equal len(persisted_roles) ({len(expected['persisted_roles'])})"
        )


def replay(data: dict[str, Any]) -> ReplayResult:
    """Reference replay: walk the canned conversation and capture behavior.

    A real backend would replace this with its own loop driving AIAgent
    (or hermes-agent-core). The reference implementation is intentionally
    independent of run_agent.py so the fixtures stay backend-neutral.
    """
    inputs = data["inputs"]
    canned_responses = list(inputs["canned_model_responses"])
    canned_tool_results = dict(inputs.get("canned_tool_results") or {})

    result = ReplayResult()
    persisted: list[dict[str, Any]] = []
    for user_msg in inputs["user_messages"]:
        persisted.append({"role": "user", "content": user_msg})

    for response in canned_responses:
        result.turn_count += 1
        tool_calls = response.get("tool_calls") or []
        assistant_msg: dict[str, Any] = {
            "role": "assistant",
            "content": response.get("content"),
        }
        if tool_calls:
            assistant_msg["tool_calls"] = tool_calls
        if "reasoning" in response and response["reasoning"]:
            assistant_msg["reasoning"] = response["reasoning"]
            result.reasoning_fields_present = True
        persisted.append(assistant_msg)

        for tc in tool_calls:
            result.tool_calls_dispatched.append(
                {"name": tc["name"], "argument_keys": sorted(tc["arguments"].keys())}
            )
            tool_result = canned_tool_results.get(tc["id"])
            if tool_result is None:
                error = f"missing canned result for tool_call {tc['id']}"
                result.errors.append(error)
                continue
            persisted.append(
                {
                    "role": "tool",
                    "tool_call_id": tc["id"],
                    "name": tc["name"],
                    "content": tool_result.get("content"),
                    "ok": tool_result.get("ok", True),
                }
            )
            result.tool_results_persisted.append(
                {"call_id": tc["id"], "ok": tool_result.get("ok", True)}
            )

    if persisted and persisted[-1]["role"] == "assistant":
        result.final_message = {
            "role": "assistant",
            "content": persisted[-1].get("content"),
        }
    result.persisted_messages = persisted
    return result


def assert_replay_matches_expected(
    expected: dict[str, Any], result: ReplayResult, *, source: str = "<fixture>"
) -> None:
    """Compare a ReplayResult against the fixture's expected section."""

    def fail(msg: str) -> None:
        raise AssertionError(f"{source}: {msg}")

    if result.turn_count != expected["turn_count"]:
        fail(
            f"turn_count: expected {expected['turn_count']}, got {result.turn_count}"
        )
    actual_persisted_count = len(result.persisted_messages)
    if actual_persisted_count != expected["persisted_message_count"]:
        fail(
            f"persisted_message_count: expected "
            f"{expected['persisted_message_count']}, got "
            f"{actual_persisted_count}"
        )
    actual_roles = [m["role"] for m in result.persisted_messages]
    if actual_roles != expected["persisted_roles"]:
        fail(f"persisted_roles: expected {expected['persisted_roles']}, got {actual_roles}")
    if result.reasoning_fields_present != expected["reasoning_fields_present"]:
        fail(
            f"reasoning_fields_present: expected "
            f"{expected['reasoning_fields_present']}, got "
            f"{result.reasoning_fields_present}"
        )

    expected_tool_calls = expected["tool_calls_dispatched"]
    if len(expected_tool_calls) != len(result.tool_calls_dispatched):
        fail(
            f"tool_calls_dispatched count: expected {len(expected_tool_calls)}, "
            f"got {len(result.tool_calls_dispatched)}"
        )
    for i, (want, got) in enumerate(zip(expected_tool_calls, result.tool_calls_dispatched)):
        if want["name"] != got["name"]:
            fail(
                f"tool_call[{i}].name: expected {want['name']}, got {got['name']}"
            )
        want_keys = sorted(want["argument_keys"])
        if want_keys != got["argument_keys"]:
            fail(
                f"tool_call[{i}].argument_keys: expected {want_keys}, "
                f"got {got['argument_keys']}"
            )

    expected_results = expected["tool_results_persisted"]
    if len(expected_results) != len(result.tool_results_persisted):
        fail(
            f"tool_results_persisted count: expected {len(expected_results)}, "
            f"got {len(result.tool_results_persisted)}"
        )
    for i, (want, got) in enumerate(zip(expected_results, result.tool_results_persisted)):
        if want["call_id"] != got["call_id"]:
            fail(
                f"tool_result[{i}].call_id: expected {want['call_id']}, "
                f"got {got['call_id']}"
            )
        if want["ok"] != got["ok"]:
            fail(
                f"tool_result[{i}].ok: expected {want['ok']}, got {got['ok']}"
            )

    final_expected = expected["final_message"]
    if final_expected is None:
        if result.final_message is not None:
            fail(f"final_message: expected None, got {result.final_message}")
    else:
        if result.final_message is None:
            fail("final_message: expected an assistant message, got None")
        actual_content = result.final_message.get("content") or ""
        contains = final_expected.get("content_contains")
        if contains and contains not in actual_content:
            fail(
                f"final_message.content does not contain "
                f"{contains!r}; got {actual_content!r}"
            )

    if list(result.errors) != list(expected["errors"]):
        fail(f"errors: expected {expected['errors']}, got {result.errors}")

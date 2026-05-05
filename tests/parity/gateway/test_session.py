"""Rust/Python parity for gateway session guard and FIFO queue helpers."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_GATEWAY_CRATE = REPO_ROOT / "crates" / "hermes-gateway"


pytestmark = pytest.mark.skipif(
    not RUST_GATEWAY_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-gateway session guard not yet built; tracked by hermes-4ne.1",
)


def _rust_trace() -> dict:
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
        f"Rust gateway snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)["session_guard_trace"]


def _python_fifo_trace() -> dict:
    from gateway.run import GatewayRunner
    from gateway.platforms.base import MessageEvent

    runner = GatewayRunner.__new__(GatewayRunner)
    runner._queued_events = {}
    adapter = SimpleNamespace(_pending_messages={})
    session_key = "agent:main:telegram:dm:42"

    runner._enqueue_fifo(session_key, MessageEvent(text="first"), adapter)
    depth_after_first_enqueue = runner._queue_depth(session_key, adapter=adapter)
    runner._enqueue_fifo(session_key, MessageEvent(text="second"), adapter)
    depth_after_second_enqueue = runner._queue_depth(session_key, adapter=adapter)
    first_pending_text = adapter._pending_messages[session_key].text

    drained = adapter._pending_messages.pop(session_key, None)
    promoted = runner._promote_queued_event(session_key, adapter, drained)

    return {
        "depth_after_first_enqueue": depth_after_first_enqueue,
        "depth_after_second_enqueue": depth_after_second_enqueue,
        "first_pending_text": first_pending_text,
        "promoted_pending_text": promoted.text if promoted else None,
        "staged_pending_text": adapter._pending_messages.get(session_key).text
        if session_key in adapter._pending_messages
        else None,
    }


def test_rust_gateway_session_fifo_matches_python_runner_helpers() -> None:
    rust = _rust_trace()
    python = _python_fifo_trace()

    for key, value in python.items():
        assert rust[key] == value


def test_rust_gateway_session_guard_lifecycle_and_bypass_smoke() -> None:
    trace = _rust_trace()
    assert trace["active_during_drain"] is True
    assert trace["active_after_empty_finish"] is False
    assert trace["stop_bypasses_busy_guard"] is True
    assert trace["status_bypasses_busy_guard"] is True
    assert trace["plain_message_queued"] is True

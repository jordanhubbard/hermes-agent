"""Rust/Python parity for gateway streaming and delivery planning contracts."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path
from unittest.mock import AsyncMock, patch
import asyncio

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_GATEWAY_CRATE = REPO_ROOT / "crates" / "hermes-gateway"

PLAIN = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda"
CODE = "Before\n```python\nprint('hello')\nprint('world')\nprint('again')\n```\nAfter"
INLINE = "Prefix text with `inline code that should not be split (inside)` and suffix text."
UTF16 = ("Hello 😀 world 🎵 test 𝄞 " * 8).strip()
DISPLAY = "Here is the file:\nMEDIA:/tmp/out.png\n[[audio_as_voice]]\nDone"


pytestmark = pytest.mark.skipif(
    not RUST_GATEWAY_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-gateway streaming delivery not yet built; tracked by hermes-4ne.3",
)


def _rust_streaming() -> dict:
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
    return json.loads(result.stdout)["streaming_delivery"]


def _python_streaming() -> dict:
    from gateway.platforms.base import BasePlatformAdapter, utf16_len
    from gateway.runtime_footer import format_runtime_footer
    from gateway.stream_consumer import GatewayStreamConsumer

    return {
        "truncated_plain": BasePlatformAdapter.truncate_message(PLAIN, 36),
        "truncated_code": BasePlatformAdapter.truncate_message(CODE, 44),
        "truncated_inline": BasePlatformAdapter.truncate_message(INLINE, 42),
        "truncated_utf16": BasePlatformAdapter.truncate_message(UTF16, 50, len_fn=utf16_len),
        "cleaned_display": GatewayStreamConsumer._clean_for_display(DISPLAY),
        "thread_metadata": {"thread_id": "topic-1"},
        "runtime_footer": format_runtime_footer(
            model="openrouter/openai/gpt-5.4",
            context_tokens=68_000,
            context_length=100_000,
            cwd="/opt/project",
            fields=("model", "context_pct", "cwd"),
        ),
        "retry_decisions": _python_retry_decisions(),
        "send_retry_plans": _python_send_retry_plans(),
        "fresh_final_success": {
            "should_send_fresh_final": True,
            "calls": ["send_initial_preview", "send_fresh_final", "delete_initial_preview"],
            "final_message_id": "fresh_final",
            "fallback_to_edit": False,
        },
        "fresh_final_send_failure": {
            "should_send_fresh_final": True,
            "calls": ["send_initial_preview", "send_fresh_final", "edit_final"],
            "final_message_id": "initial_preview",
            "fallback_to_edit": True,
        },
        "fresh_final_disabled": {
            "should_send_fresh_final": False,
            "calls": ["send_initial_preview", "edit_final"],
            "final_message_id": "initial_preview",
            "fallback_to_edit": False,
        },
        "fresh_final_without_delete": {
            "should_send_fresh_final": True,
            "calls": ["send_initial_preview", "send_fresh_final"],
            "final_message_id": "fresh_final",
            "fallback_to_edit": False,
        },
        "fresh_final_short_lived": {
            "should_send_fresh_final": False,
            "calls": ["send_initial_preview", "edit_final"],
            "final_message_id": "initial_preview",
            "fallback_to_edit": False,
        },
        "fresh_final_nonfinal": {
            "should_send_fresh_final": False,
            "calls": ["send_initial_preview", "edit_final"],
            "final_message_id": "initial_preview",
            "fallback_to_edit": False,
        },
    }


def _python_retry_decisions() -> dict:
    from gateway.platforms.base import BasePlatformAdapter

    samples = (
        "httpx.ConnectError: connection dropped",
        "Forbidden: bot was blocked by the user",
        "Bad Request: can't parse entities",
        "CONNECTERROR: host unreachable",
        "ReadTimeout: request timed out",
        "Timed out waiting for response",
        "ConnectTimeout: connection timed out",
        "internal platform error",
    )
    decisions = {}
    for error in samples:
        retryable = BasePlatformAdapter._is_retryable_error(error)
        timeout = BasePlatformAdapter._is_timeout_error(error)
        if retryable:
            action = "retry"
        elif timeout:
            action = "return_failure_no_retry"
        else:
            action = "plain_text_fallback"
        decisions[error] = {
            "retryable": retryable,
            "timeout": timeout,
            "action": action,
        }
    return decisions


def _python_send_retry_plans() -> dict:
    from gateway.platforms.base import BasePlatformAdapter, SendResult
    from gateway.config import Platform, PlatformConfig

    class StubAdapter(BasePlatformAdapter):
        def __init__(self, results):
            super().__init__(PlatformConfig(), Platform.TELEGRAM)
            self._send_results = list(results)
            self._send_calls = []

        async def send(self, chat_id, content, reply_to=None, metadata=None, **kwargs):
            self._send_calls.append((chat_id, content))
            if self._send_results:
                return self._send_results.pop(0)
            return SendResult(success=True, message_id="ok")

        async def connect(self):
            return True

        async def disconnect(self):
            pass

        async def send_typing(self, chat_id, metadata=None):
            pass

        async def get_chat_info(self, chat_id):
            return {"name": "test", "type": "direct", "chat_id": chat_id}

    async def run_case(results, max_retries):
        adapter = StubAdapter(results)
        with patch("asyncio.sleep", new_callable=AsyncMock):
            result = await adapter._send_with_retry("chat1", "hello", max_retries=max_retries, base_delay=0)
        return {
            "success": result.success,
            "send_calls": len(adapter._send_calls),
            "message_id": result.message_id,
            "final_error": result.error,
            "fallback_sent": any("plain text" in content.lower() for _, content in adapter._send_calls),
            "notice_sent": any("delivery failed" in content.lower() for _, content in adapter._send_calls),
        }

    network_err = SendResult(success=False, error="httpx.ConnectError: host unreachable")
    return asyncio.run(_collect_retry_plans(run_case, network_err, SendResult))


async def _collect_retry_plans(run_case, network_err, send_result_cls) -> dict:
    return {
        "success_first_attempt": await run_case([send_result_cls(success=True, message_id="123")], 2),
        "connect_error_then_success": await run_case(
            [network_err, send_result_cls(success=True, message_id="ok")], 2
        ),
        "read_timeout_not_retried": await run_case(
            [send_result_cls(success=False, error="ReadTimeout: request timed out")], 3
        ),
        "retryable_flag_then_success": await run_case(
            [
                send_result_cls(success=False, error="internal platform error", retryable=True),
                send_result_cls(success=True, message_id="ok"),
            ],
            2,
        ),
        "network_to_formatting_fallback": await run_case(
            [
                network_err,
                send_result_cls(success=False, error="Bad Request: can't parse entities"),
                send_result_cls(success=True, message_id="fallback_ok"),
            ],
            2,
        ),
        "network_exhausted_notice": await run_case(
            [network_err, network_err, network_err, send_result_cls(success=True)],
            2,
        ),
    }


def _assert_chunk_indicators(chunks: list[str]) -> None:
    total = len(chunks)
    assert total > 1
    for idx, chunk in enumerate(chunks, start=1):
        assert chunk.endswith(f"({idx}/{total})")


def test_rust_streaming_delivery_snapshot_matches_python_helpers() -> None:
    assert _rust_streaming() == _python_streaming()


def test_rust_streaming_delivery_covers_required_contracts() -> None:
    snapshot = _rust_streaming()
    _assert_chunk_indicators(snapshot["truncated_plain"])
    _assert_chunk_indicators(snapshot["truncated_utf16"])
    assert all(chunk.count("```") % 2 == 0 for chunk in snapshot["truncated_code"])
    assert "MEDIA:" not in snapshot["cleaned_display"]
    assert snapshot["thread_metadata"] == {"thread_id": "topic-1"}
    assert snapshot["runtime_footer"] == "gpt-5.4 · 68% · /opt/project"
    assert snapshot["retry_decisions"]["ReadTimeout: request timed out"]["action"] == "return_failure_no_retry"
    assert snapshot["send_retry_plans"]["connect_error_then_success"]["send_calls"] == 2
    assert snapshot["send_retry_plans"]["network_exhausted_notice"]["notice_sent"] is True
    assert snapshot["fresh_final_success"]["calls"][-1] == "delete_initial_preview"
    assert snapshot["fresh_final_send_failure"]["fallback_to_edit"] is True
    assert snapshot["fresh_final_without_delete"]["calls"] == ["send_initial_preview", "send_fresh_final"]

"""End-to-end smoke for the Rust state stack via the production factory.

Drives ``hermes_state_factory.get_session_db`` with
``HERMES_STATE_BACKEND=rust`` and ``HERMES_STATE_BOUNDARY=daemon`` —
the same env-shape an operator would set — and runs a realistic
session lifecycle through the daemon. This exercises every layer the
real entry points (CLI, gateway, dashboard) hit:

  env / config → factory → RustSessionDB → _DaemonClient → daemon
  binary (UDS framed JSON) → SessionStore → SQLite

Bead ``hermes-te4.3`` (exercise Rust state backend in real production
entry points). Skipped when ``cargo`` is unavailable since the daemon
binary needs to be built (or already present).
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest

import hermes_state_factory as factory
from hermes_state_rust import RustSessionDB


pytestmark = pytest.mark.skipif(
    shutil.which("cargo") is None,
    reason="cargo not installed; daemon binary cannot be built",
)


@pytest.fixture(autouse=True)
def _reset_factory_and_env(monkeypatch):
    monkeypatch.setenv(factory.ENV_VAR, "rust")
    monkeypatch.setenv("HERMES_STATE_BOUNDARY", "daemon")
    factory.reset_selection_for_tests()
    yield
    factory.reset_selection_for_tests()


def test_factory_returns_daemon_backed_rust_db(tmp_path: Path) -> None:
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        assert isinstance(db, RustSessionDB)
        assert db.boundary == "daemon"
    finally:
        db.close()
    snap = factory.state_backend_diagnostics()
    assert snap["backend"] == "rust"
    assert snap["source"] == "env"
    assert snap["fallback_reason"] is None


def test_realistic_session_lifecycle_via_daemon(tmp_path: Path) -> None:
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        # Create a session as the gateway/CLI would.
        session_id = db.create_session(
            "lifecycle-1",
            source="cli",
            model="gpt-4o",
            system_prompt="you are hermes",
        )
        assert session_id == "lifecycle-1"

        # Append a representative message sequence: user, assistant w/
        # tool call, tool result, assistant w/ reasoning, final.
        db.append_message(
            session_id, role="user", content="Read README.md"
        )
        db.append_message(
            session_id,
            role="assistant",
            tool_calls=[
                {
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": '{"path": "README.md"}',
                    },
                }
            ],
        )
        db.append_message(
            session_id,
            role="tool",
            content="Hermes Agent\nA local-first AI agent.",
            tool_name="read_file",
            tool_call_id="call_1",
        )
        db.append_message(
            session_id,
            role="assistant",
            content="The README begins with 'Hermes Agent'.",
            reasoning="The first line of the file is 'Hermes Agent'.",
        )

        # Token counts as a real turn would update them.
        db.update_token_counts(
            session_id,
            input_tokens=42,
            output_tokens=17,
            cache_read_tokens=0,
            cache_write_tokens=0,
            reasoning_tokens=8,
        )

        # Read the messages back — both the canonical method and the
        # conversation-flattened method (which the agent loop uses).
        messages = db.get_messages(session_id)
        assert len(messages) == 4
        roles = [m["role"] for m in messages]
        assert roles == ["user", "assistant", "tool", "assistant"]
        assert messages[2]["tool_call_id"] == "call_1"
        assert messages[3]["reasoning"]

        # FTS search the way the session_search tool does.
        matches = db.search_messages("Hermes")
        assert any(m["session_id"] == session_id for m in matches)

        # Rich session listing the way the dashboard / sessions list uses.
        rows = db.list_sessions_rich(limit=10)
        assert any(r["id"] == session_id for r in rows)

        # End the session as a normal turn finalize would.
        db.end_session(session_id, end_reason="completed")
        finalized = db.get_session(session_id)
        assert finalized is not None
        assert finalized["ended_at"] is not None
        assert finalized["end_reason"] == "completed"
        # Token rollup landed.
        assert finalized["input_tokens"] == 42
        assert finalized["output_tokens"] == 17
        assert finalized["reasoning_tokens"] == 8

        # Delete cleanup, the way `hermes sessions delete` does.
        assert db.delete_session(session_id) is True
        assert db.get_session(session_id) is None

        # Diagnostics confirm we ran through the daemon, not the cargo
        # subprocess fallback.
        adapter_snap = db.diagnostics()
        assert adapter_snap["boundary"] == "daemon"
        assert adapter_snap["error_count"] == 0
        assert adapter_snap["op_count"] >= 10
    finally:
        db.close()


def test_unified_diagnostics_merges_adapter(tmp_path: Path) -> None:
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        snapshot = factory.state_backend_diagnostics(db)
    finally:
        db.close()
    assert snapshot["backend"] == "rust"
    assert "adapter" in snapshot
    assert snapshot["adapter"]["boundary"] == "daemon"
    assert snapshot["adapter"]["backend"] == "rust"

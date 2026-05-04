"""Parity tests for the Python-shaped RustSessionDB adapter."""

import sqlite3
import time

import pytest

from hermes_state import SessionDB
from hermes_state_rust import RustSessionDB, RustStateBackendError, get_state_db_class


def _db_pair(tmp_path):
    py_db = SessionDB(tmp_path / "python_state.db")
    try:
        rust_db = RustSessionDB(tmp_path / "rust_state.db")
    except RustStateBackendError as exc:
        py_db.close()
        pytest.skip(str(exc))
    return py_db, rust_db


def _close_pair(py_db, rust_db):
    py_db.close()
    rust_db.close()


def _selected_session(row):
    return {
        key: row[key]
        for key in [
            "id",
            "source",
            "user_id",
            "model",
            "model_config",
            "system_prompt",
            "parent_session_id",
            "end_reason",
            "message_count",
            "tool_call_count",
            "input_tokens",
            "output_tokens",
            "cache_read_tokens",
            "cache_write_tokens",
            "reasoning_tokens",
            "billing_provider",
            "billing_base_url",
            "billing_mode",
            "estimated_cost_usd",
            "actual_cost_usd",
            "cost_status",
            "cost_source",
            "pricing_version",
            "api_call_count",
        ]
    }


def test_backend_class_switch(monkeypatch):
    assert get_state_db_class("rust") is RustSessionDB

    monkeypatch.setenv("HERMES_STATE_BACKEND", "rust")
    assert get_state_db_class() is RustSessionDB

    monkeypatch.setenv("HERMES_STATE_BACKEND", "python")
    assert get_state_db_class() is SessionDB


def test_rust_adapter_matches_session_lifecycle_and_tokens(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            assert db.create_session(
                "s1",
                "cli",
                model="model-a",
                model_config={"temperature": 0.2},
                system_prompt="system",
                user_id="user-1",
            ) == "s1"
            db.update_system_prompt("s1", "system v2")
            db.update_token_counts(
                "s1",
                input_tokens=10,
                output_tokens=5,
                cache_read_tokens=2,
                cache_write_tokens=3,
                reasoning_tokens=4,
                estimated_cost_usd=0.25,
                actual_cost_usd=0.20,
                cost_status="estimated",
                cost_source="unit-test",
                pricing_version="v1",
                billing_provider="provider",
                billing_base_url="https://example.test",
                billing_mode="test",
                model="model-b",
                api_call_count=1,
            )
            db.update_token_counts(
                "s1",
                input_tokens=1,
                output_tokens=2,
                actual_cost_usd=0.05,
                api_call_count=2,
            )
            db.end_session("s1", "compression")
            db.end_session("s1", "ignored")
            db.reopen_session("s1")
            db.end_session("s1", "user_exit")

        assert _selected_session(py_db.get_session("s1")) == _selected_session(
            rust_db.get_session("s1")
        )
        assert rust_db.get_session("missing") is None
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_message_replay_and_replace(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    details = [{"type": "reasoning.summary", "summary": "Thinking about tools"}]
    tool_calls = [
        {"id": "c1", "type": "function", "function": {"name": "date", "arguments": "{}"}}
    ]
    multimodal = [
        {"type": "text", "text": "describe this screenshot"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAA"}},
    ]
    try:
        for db in (py_db, rust_db):
            db.create_session("root", "tui")
            db.append_message("root", role="user", content="same prompt")
            db.append_message("root", role="user", content="same prompt")
            db.append_message("root", role="assistant", content="answer")
            db.create_session("child", "tui", parent_session_id="root")
            db.append_message("child", role="user", content=multimodal)
            db.append_message(
                "child",
                role="assistant",
                content=(
                    "<memory-context>stale</memory-context>\n"
                    "Visible answer"
                ),
                tool_calls=tool_calls,
                finish_reason="tool_calls",
                reasoning="I should call a tool",
                reasoning_content="",
                reasoning_details=details,
            )
            db.append_message("child", role="tool", content="ok", tool_call_id="c1")

        assert [
            (message["role"], message["content"], message.get("tool_calls"))
            for message in py_db.get_messages("child")
        ] == [
            (message["role"], message["content"], message.get("tool_calls"))
            for message in rust_db.get_messages("child")
        ]

        py_conv = py_db.get_messages_as_conversation("child", include_ancestors=True)
        rust_conv = rust_db.get_messages_as_conversation("child", include_ancestors=True)
        assert py_conv == rust_conv
        assert [msg["content"] for msg in rust_conv if msg["role"] == "user"] == [
            "same prompt",
            multimodal,
        ]

        replacement = [
            {"role": "user", "content": "replacement"},
            {"role": "assistant", "content": "", "tool_calls": tool_calls},
        ]
        for db in (py_db, rust_db):
            db.replace_messages("child", replacement)

        assert py_db.get_messages_as_conversation(
            "child"
        ) == rust_db.get_messages_as_conversation("child")
        assert _selected_session(py_db.get_session("child")) == _selected_session(
            rust_db.get_session("child")
        )
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_search_surface(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            db.create_session("s1", "cli")
            db.append_message("s1", role="user", content="before needle")
            db.append_message("s1", role="assistant", content="Use docker compose up.")
            db.append_message("s1", role="user", content="after needle")
            db.create_session("s2", "telegram")
            db.append_message("s2", role="user", content="今天讨论A2A通信协议的实现")
            db.append_message(
                "s2",
                role="assistant",
                content="",
                tool_name="UNIQUETOOLNAME",
                tool_calls=[
                    {
                        "id": "c1",
                        "type": "function",
                        "function": {
                            "name": "UNIQUEFUNCNAME",
                            "arguments": "{\"query\": \"UNIQUESEARCHTOKEN\"}",
                        },
                    }
                ],
            )

        assert _search_projection(py_db.search_messages("docker")) == _search_projection(
            rust_db.search_messages("docker")
        )
        assert _search_projection(py_db.search_messages("通信")) == _search_projection(
            rust_db.search_messages("通信")
        )
        assert py_db.search_messages("通信", source_filter=["cli"]) == rust_db.search_messages(
            "通信", source_filter=["cli"]
        )
        assert _search_projection(
            py_db.search_messages("UNIQUETOOLNAME", role_filter=["assistant"])
        ) == _search_projection(
            rust_db.search_messages("UNIQUETOOLNAME", role_filter=["assistant"])
        )
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_listing_counts_and_exports(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            db.create_session("s1", "cli", model="test-model")
            db.create_session("s2", "telegram")
            db.create_session("s3", "cli")
            db.append_message("s1", role="user", content="A")
            db.append_message("s1", role="assistant", content="B")
            db.append_message("s2", role="user", content="C")

        assert py_db.session_count() == rust_db.session_count() == 3
        assert py_db.session_count(source="cli") == rust_db.session_count(source="cli") == 2
        assert py_db.message_count() == rust_db.message_count() == 3
        assert (
            py_db.message_count(session_id="s1")
            == rust_db.message_count(session_id="s1")
            == 2
        )

        assert _session_list_projection(py_db.search_sessions()) == _session_list_projection(
            rust_db.search_sessions()
        )
        assert _session_list_projection(
            py_db.search_sessions(source="cli", limit=1, offset=1)
        ) == _session_list_projection(
            rust_db.search_sessions(source="cli", limit=1, offset=1)
        )

        py_export = py_db.export_session("s1")
        rust_export = rust_db.export_session("s1")
        assert _export_projection(py_export) == _export_projection(rust_export)
        assert rust_db.export_session("missing") is None

        assert _export_all_projection(py_db.export_all(source="cli")) == _export_all_projection(
            rust_db.export_all(source="cli")
        )
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_clear_delete_and_prefix_resolution(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            db.create_session("20260315_092437_c9a6ff", "cli")
            db.create_session("20260315X092437_c9a6ff", "cli")
            db.create_session("20260315_092437_c9a6aa", "telegram")
            db.create_session("parent", "cli")
            db.create_session("child", "cli", parent_session_id="parent")
            db.append_message("parent", role="user", content="parent")
            db.append_message("child", role="user", content="child")

        assert py_db.resolve_session_id("20260315X092437_c9a6") == rust_db.resolve_session_id(
            "20260315X092437_c9a6"
        )
        assert py_db.resolve_session_id("20260315_092437_c9a6") == rust_db.resolve_session_id(
            "20260315_092437_c9a6"
        )

        for db in (py_db, rust_db):
            db.clear_messages("parent")
        assert py_db.message_count("parent") == rust_db.message_count("parent") == 0
        assert _selected_session(py_db.get_session("parent")) == _selected_session(
            rust_db.get_session("parent")
        )

        assert py_db.delete_session("missing") is False
        assert rust_db.delete_session("missing") is False
        assert py_db.delete_session("parent") is True
        assert rust_db.delete_session("parent") is True
        assert py_db.get_session("parent") is None
        assert rust_db.get_session("parent") is None
        assert _selected_session(py_db.get_session("child")) == _selected_session(
            rust_db.get_session("child")
        )
        assert rust_db.get_session("child")["parent_session_id"] is None
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_title_helpers(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            db.create_session("s1", "cli")
            db.create_session("s2", "cli")
            db.create_session("s3", "cli")

        assert py_db.set_session_title("s1", "  My\tSession\nTitle  ") is True
        assert rust_db.set_session_title("s1", "  My\tSession\nTitle  ") is True
        assert py_db.get_session_title("s1") == rust_db.get_session_title("s1")
        assert _selected_session(py_db.get_session_by_title("My Session Title")) == _selected_session(
            rust_db.get_session_by_title("My Session Title")
        )

        with pytest.raises(ValueError):
            py_db.set_session_title("s2", "My Session Title")
        with pytest.raises(ValueError):
            rust_db.set_session_title("s2", "My Session Title")

        for db in (py_db, rust_db):
            db.set_session_title("s2", "My Session Title #2")
            db.set_session_title("s3", "My Session Title #3")

        assert py_db.resolve_session_by_title("My Session Title") == rust_db.resolve_session_by_title(
            "My Session Title"
        )
        assert py_db.get_next_title_in_lineage("My Session Title") == rust_db.get_next_title_in_lineage(
            "My Session Title"
        )

        assert py_db.set_session_title("s1", "   ") is True
        assert rust_db.set_session_title("s1", "   ") is True
        assert py_db.get_session_title("s1") is None
        assert rust_db.get_session_title("s1") is None
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_resume_and_compression_helpers(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        for db in (py_db, rust_db):
            db.create_session("root", "cli")
            db.create_session("empty-child", "cli", parent_session_id="root")
            db.create_session("message-child", "cli", parent_session_id="empty-child")
            db.append_message("message-child", role="user", content="hello")

        assert py_db.resolve_resume_session_id("root") == rust_db.resolve_resume_session_id("root")
        assert py_db.resolve_resume_session_id("message-child") == rust_db.resolve_resume_session_id(
            "message-child"
        )
        assert py_db.resolve_resume_session_id("missing") == rust_db.resolve_resume_session_id(
            "missing"
        )

        for db in (py_db, rust_db):
            db.create_session("croot", "cli")
            db.end_session("croot", "compression")
            db.create_session("ctip", "cli", parent_session_id="croot")

        _set_started_at(py_db, "ctip", py_db.get_session("croot")["ended_at"] + 0.001)
        _set_started_at(rust_db, "ctip", rust_db.get_session("croot")["ended_at"] + 0.001)

        assert py_db.get_compression_tip("croot") == rust_db.get_compression_tip("croot")
        assert py_db.get_compression_tip("missing") == rust_db.get_compression_tip("missing")
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_rich_listing_filters_projection_and_order(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        t0 = 1709500000.0
        for db in (py_db, rust_db):
            db.create_session("compress_root", "cli", model="root-model", system_prompt="root")
            _set_started_at(db, "compress_root", t0)
            _set_ended(db, "compress_root", t0 + 100, "compression")
            db.append_message("compress_root", role="user", content="old compression ask")
            _set_message_timestamp(db, "compress_root", "old compression ask", t0 + 1)

            db.create_session(
                "compress_tip",
                "cli",
                parent_session_id="compress_root",
                model="tip-model",
                system_prompt="tip",
            )
            _set_started_at(db, "compress_tip", t0 + 101)
            db.append_message("compress_tip", role="user", content="latest compression ask")
            _set_message_timestamp(db, "compress_tip", "latest compression ask", t0 + 10_000)
            db.set_session_title("compress_tip", "live tip")

            db.create_session("branch_parent", "cli")
            _set_started_at(db, "branch_parent", t0 + 10)
            _set_ended(db, "branch_parent", t0 + 11, "branched")
            db.create_session("branch", "cli", parent_session_id="branch_parent")
            _set_started_at(db, "branch", t0 + 12)
            db.append_message("branch", role="user", content="branch path")
            _set_message_timestamp(db, "branch", "branch path", t0 + 13)

            db.create_session("root", "cli")
            _set_started_at(db, "root", t0 + 20)
            db.append_message("root", role="user", content=("A" * 70) + "\nsecond line")
            _set_message_timestamp(db, "root", ("A" * 70) + "\nsecond line", t0 + 21)

            db.create_session("delegate", "cli", parent_session_id="root")
            _set_started_at(db, "delegate", t0 + 22)
            db.append_message("delegate", role="user", content="delegate task")
            _set_message_timestamp(db, "delegate", "delegate task", t0 + 23)

            db.create_session("tool_session", "tool")
            _set_started_at(db, "tool_session", t0 + 30)
            db.append_message("tool_session", role="user", content="tool ask")
            _set_message_timestamp(db, "tool_session", "tool ask", t0 + 31)

            db.create_session("cron_session", "cron")
            _set_started_at(db, "cron_session", t0 + 40)
            db.append_message("cron_session", role="user", content="cron ask")
            _set_message_timestamp(db, "cron_session", "cron ask", t0 + 41)

            db.create_session("newer", "cli")
            _set_started_at(db, "newer", t0 + 500)
            db.append_message("newer", role="user", content="newer ask")
            _set_message_timestamp(db, "newer", "newer ask", t0 + 500)

        assert _rich_projection(py_db.list_sessions_rich(limit=20)) == _rich_projection(
            rust_db.list_sessions_rich(limit=20)
        )

        default_ids = [row["id"] for row in rust_db.list_sessions_rich(limit=20)]
        assert "delegate" not in default_ids
        assert "compress_root" not in default_ids
        assert "compress_tip" in default_ids
        assert "branch" in default_ids
        tip_row = next(
            row
            for row in rust_db.list_sessions_rich(limit=20)
            if row["id"] == "compress_tip"
        )
        assert tip_row["_lineage_root_id"] == "compress_root"
        assert tip_row["started_at"] == t0
        assert tip_row["model"] == "tip-model"
        assert tip_row["system_prompt"] == "tip"

        assert _rich_projection(
            py_db.list_sessions_rich(limit=20, include_children=True)
        ) == _rich_projection(
            rust_db.list_sessions_rich(limit=20, include_children=True)
        )
        assert "delegate" in [
            row["id"] for row in rust_db.list_sessions_rich(limit=20, include_children=True)
        ]

        assert _rich_projection(
            py_db.list_sessions_rich(limit=20, exclude_sources=["tool", "cron"])
        ) == _rich_projection(
            rust_db.list_sessions_rich(limit=20, exclude_sources=["tool", "cron"])
        )
        assert _rich_projection(
            py_db.list_sessions_rich(source="cli", limit=20)
        ) == _rich_projection(rust_db.list_sessions_rich(source="cli", limit=20))
        assert _rich_projection(
            py_db.list_sessions_rich(limit=20, project_compression_tips=False)
        ) == _rich_projection(
            rust_db.list_sessions_rich(limit=20, project_compression_tips=False)
        )

        assert _rich_projection(
            py_db.list_sessions_rich(limit=1, order_by_last_active=True)
        ) == _rich_projection(
            rust_db.list_sessions_rich(limit=1, order_by_last_active=True)
        )
        assert (
            rust_db.list_sessions_rich(limit=1, order_by_last_active=True)[0]["id"]
            == "compress_tip"
        )
    finally:
        _close_pair(py_db, rust_db)


def test_rust_adapter_matches_prune_meta_and_auto_maintenance(tmp_path):
    py_db, rust_db = _db_pair(tmp_path)
    try:
        old_ts = time.time() - 200 * 86400
        for db in (py_db, rust_db):
            db.create_session("old_cli", "cli")
            db.end_session("old_cli", "done")
            db.create_session("old_tg", "telegram")
            db.end_session("old_tg", "done")
            db.create_session("active", "cli")
            db.create_session("child", "cli", parent_session_id="old_cli")
            db.set_meta("last_auto_prune", "123")

        for db in (py_db, rust_db):
            for session_id in ("old_cli", "old_tg", "active"):
                _set_started_at(db, session_id, old_ts)

        assert py_db.get_meta("last_auto_prune") == rust_db.get_meta("last_auto_prune") == "123"
        py_db.set_meta("last_auto_prune", "456")
        rust_db.set_meta("last_auto_prune", "456")
        assert py_db.get_meta("last_auto_prune") == rust_db.get_meta("last_auto_prune") == "456"

        assert py_db.prune_sessions(older_than_days=90, source="cli") == rust_db.prune_sessions(
            older_than_days=90, source="cli"
        ) == 1
        assert py_db.get_session("old_cli") is None
        assert rust_db.get_session("old_cli") is None
        assert _selected_session(py_db.get_session("child")) == _selected_session(
            rust_db.get_session("child")
        )
        assert rust_db.get_session("child")["parent_session_id"] is None
        assert py_db.get_session("old_tg") is not None
        assert rust_db.get_session("old_tg") is not None

        for db in (py_db, rust_db):
            db.create_session("ghost", "tui")
            db.end_session("ghost", "user_exit")
            _set_started_at(db, "ghost", old_ts)
        assert py_db.prune_empty_ghost_sessions() == rust_db.prune_empty_ghost_sessions() == 1

        py_db.set_meta("last_auto_prune", str(time.time()))
        rust_db.set_meta("last_auto_prune", py_db.get_meta("last_auto_prune"))
        assert py_db.maybe_auto_prune_and_vacuum(vacuum=False) == rust_db.maybe_auto_prune_and_vacuum(
            vacuum=False
        )
    finally:
        _close_pair(py_db, rust_db)


def _search_projection(results):
    return [
        {
            "session_id": result["session_id"],
            "role": result["role"],
            "tool_name": result.get("tool_name"),
            "source": result["source"],
            "context": [
                (message["role"], message["content"])
                for message in result.get("context", [])
            ],
        }
        for result in results
    ]


def _session_list_projection(results):
    return [
        {
            "id": result["id"],
            "source": result["source"],
            "message_count": result["message_count"],
            "tool_call_count": result["tool_call_count"],
        }
        for result in results
    ]


def _rich_projection(results):
    return [
        {
            "id": result["id"],
            "source": result["source"],
            "model": result.get("model"),
            "system_prompt": result.get("system_prompt"),
            "message_count": result["message_count"],
            "tool_call_count": result["tool_call_count"],
            "title": result.get("title"),
            "preview": result["preview"],
            "end_reason": result.get("end_reason"),
            "_lineage_root_id": result.get("_lineage_root_id"),
        }
        for result in results
    ]


def _export_projection(result):
    if result is None:
        return None
    return {
        "id": result["id"],
        "source": result["source"],
        "model": result.get("model"),
        "messages": [
            (message["role"], message["content"], message.get("tool_calls"))
            for message in result["messages"]
        ],
    }


def _export_all_projection(results):
    return [_export_projection(result) for result in results]


def _set_started_at(db, session_id, started_at):
    _execute_state_sql(
        db,
        "UPDATE sessions SET started_at = ? WHERE id = ?",
        (started_at, session_id),
    )


def _set_ended(db, session_id, ended_at, end_reason):
    _execute_state_sql(
        db,
        "UPDATE sessions SET ended_at = ?, end_reason = ? WHERE id = ?",
        (ended_at, end_reason, session_id),
    )


def _set_message_timestamp(db, session_id, content, timestamp):
    _execute_state_sql(
        db,
        "UPDATE messages SET timestamp = ? WHERE session_id = ? AND content = ?",
        (timestamp, session_id, content),
    )


def _execute_state_sql(db, sql, params):
    if isinstance(db, RustSessionDB):
        conn = sqlite3.connect(db.db_path)
        try:
            conn.execute(sql, params)
            conn.commit()
        finally:
            conn.close()
    else:
        db._conn.execute(sql, params)
        db._conn.commit()

"""Diagnostics tests for the Rust state adapter.

Covers bead hermes-izz.3 (state backend observability + rollback diagnostics).
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from hermes_state_rust import RustSessionDB, RustStateBackendError


pytestmark = pytest.mark.skipif(
    shutil.which("cargo") is None,
    reason="cargo not installed; Rust adapter unavailable",
)


def test_diagnostics_includes_required_keys(tmp_path: Path) -> None:
    db = RustSessionDB(tmp_path / "state.db")
    try:
        snap = db.diagnostics()
    finally:
        db.close()

    assert snap["backend"] == "rust"
    assert snap["boundary"] == "cargo-subprocess"
    assert snap["db_path"] == str(tmp_path / "state.db")
    assert isinstance(snap["schema_version"], int) and snap["schema_version"] > 0
    assert snap["migration_action"] == "schema_checked"
    # op_count is at least 1 because __init__ calls schema_version.
    assert snap["op_count"] >= 1
    assert snap["error_count"] == 0
    assert snap["last_error"] is None
    assert snap["last_error_class"] is None


def test_diagnostics_op_count_grows(tmp_path: Path) -> None:
    db = RustSessionDB(tmp_path / "state.db")
    try:
        before = db.diagnostics()["op_count"]
        db.create_session("session-a", source="cli")
        after = db.diagnostics()["op_count"]
    finally:
        db.close()
    assert after > before


def test_diagnostics_records_errors(tmp_path: Path, monkeypatch) -> None:
    """A probe failure should bump error_count and surface last_error."""
    db = RustSessionDB(tmp_path / "state.db")
    try:
        # Force the next probe call to fail by pointing cargo at /nonexistent.
        db.cargo = "/nonexistent/cargo"
        with pytest.raises(RustStateBackendError):
            db.create_session("session-b", source="cli")
        snap = db.diagnostics()
    finally:
        # Restore cargo so close() doesn't error.
        db._conn = None  # type: ignore[attr-defined]
    assert snap["error_count"] >= 1
    assert snap["last_error"]
    assert snap["last_error_class"] == "FileNotFoundError"


def test_rollback_diagnostics_confirm_python_can_open_rust_db(tmp_path: Path) -> None:
    db = RustSessionDB(tmp_path / "state.db")
    try:
        db.create_session("rollback-safe", source="cli")
        snap = db.rollback_diagnostics()
    finally:
        db.close()

    assert snap["python_readable"] is True
    assert snap["db_path"] == str(tmp_path / "state.db")
    assert isinstance(snap["schema_version"], int) and snap["schema_version"] > 0
    assert snap["session_count"] == 1
    assert snap["error"] is None
    assert snap["error_class"] is None


def test_initialization_logs_schema_and_db_path(tmp_path: Path, caplog) -> None:
    caplog.set_level("INFO", logger="hermes_state_rust")
    db = RustSessionDB(tmp_path / "state.db")
    try:
        pass
    finally:
        db.close()

    messages = [record.getMessage() for record in caplog.records]
    assert any("rust state backend initialized" in message for message in messages)
    assert any(str(tmp_path / "state.db") in message for message in messages)
    assert any("schema_version=" in message for message in messages)
    assert any("migration_action=schema_checked" in message for message in messages)

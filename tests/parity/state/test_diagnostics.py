"""Diagnostics tests for the Rust state adapter.

Covers bead hermes-izz.3 (state backend observability + rollback diagnostics).
The full rollback story belongs to a future bead — this file pins the
diagnostic surface so observability cannot be silently regressed.
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
    # op_count is at least 1 because __init__ calls schema_version.
    assert snap["op_count"] >= 1
    assert snap["error_count"] == 0
    assert snap["last_error"] is None


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

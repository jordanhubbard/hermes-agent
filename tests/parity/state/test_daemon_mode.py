"""End-to-end test for the Rust state daemon boundary.

Constructs ``RustSessionDB(boundary="daemon")``, which autospawns the
``hermes_state_daemon`` binary, connects over a Unix socket, and routes
ops through the framed JSON protocol. Bead ``hermes-izz.1``.

Skipped on platforms where ``cargo`` is unavailable, since the daemon
binary needs to be built (or already present) for the test to run.
"""

from __future__ import annotations

import shutil
import time
from pathlib import Path

import pytest

from hermes_state_rust import RustSessionDB, RustStateBackendError


pytestmark = pytest.mark.skipif(
    shutil.which("cargo") is None,
    reason="cargo not installed; daemon binary cannot be built",
)


@pytest.fixture()
def daemon_db(tmp_path: Path):
    db = RustSessionDB(tmp_path / "state.db", boundary="daemon")
    try:
        yield db
    finally:
        db.close()


def test_diagnostics_report_daemon_boundary(daemon_db: RustSessionDB) -> None:
    snap = daemon_db.diagnostics()
    assert snap["backend"] == "rust"
    assert snap["boundary"] == "daemon"
    assert snap["op_count"] >= 1  # __init__ called schema_version
    assert snap["error_count"] == 0


def test_create_and_get_session_via_daemon(daemon_db: RustSessionDB) -> None:
    session_id = daemon_db.create_session("s-daemon-1", source="cli", model="x")
    assert session_id == "s-daemon-1"
    row = daemon_db.get_session("s-daemon-1")
    assert row is not None
    assert row["id"] == "s-daemon-1"
    assert row["source"] == "cli"
    assert row["model"] == "x"


def test_repeated_ops_share_one_daemon_connection(daemon_db: RustSessionDB) -> None:
    """30 ops on one adapter should be served by one daemon round-trip socket."""
    for i in range(30):
        daemon_db.create_session(f"s-{i:02d}", source="cli")
    snap = daemon_db.diagnostics()
    # __init__ is 1 op, plus 30 create_session ops = 31. Subprocess mode
    # would have spawned cargo 30 extra times; daemon mode reuses the
    # connection, so this test would be slow under the old boundary.
    assert snap["op_count"] >= 31
    assert snap["error_count"] == 0


def test_daemon_error_propagates_as_backend_error(daemon_db: RustSessionDB) -> None:
    # Force an unknown op via the private dispatcher.
    with pytest.raises(RustStateBackendError):
        daemon_db._run_operation({"op": "definitely-not-a-real-op"})
    snap = daemon_db.diagnostics()
    assert snap["error_count"] >= 1
    assert "definitely-not-a-real-op" in (snap["last_error"] or "")


def test_invalid_boundary_value_raises(tmp_path: Path) -> None:
    with pytest.raises(RustStateBackendError, match="unsupported boundary"):
        RustSessionDB(tmp_path / "x.db", boundary="not-a-mode")


def test_env_var_selects_daemon_boundary(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv("HERMES_STATE_BOUNDARY", "daemon")
    db = RustSessionDB(tmp_path / "state.db")
    try:
        assert db.boundary == "daemon"
        assert db.diagnostics()["boundary"] == "daemon"
    finally:
        db.close()


def test_subprocess_boundary_remains_default(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.delenv("HERMES_STATE_BOUNDARY", raising=False)
    db = RustSessionDB(tmp_path / "state.db")
    try:
        assert db.boundary == "subprocess"
        assert db.diagnostics()["boundary"] == "cargo-subprocess"
    finally:
        db.close()

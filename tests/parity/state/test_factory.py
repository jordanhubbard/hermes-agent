"""Tests for hermes_state_factory.

Covers bead hermes-te4.1 (factory) and hermes-te4.2 (selection + diagnostics).
"""

from __future__ import annotations

from pathlib import Path
from unittest import mock

import pytest

import hermes_state_factory as factory
from hermes_state import SessionDB


@pytest.fixture(autouse=True)
def _reset_factory(monkeypatch):
    monkeypatch.delenv(factory.ENV_VAR, raising=False)
    factory.reset_selection_for_tests()
    yield
    factory.reset_selection_for_tests()


def _stub_load_config(monkeypatch, value):
    """Stub hermes_cli.config.load_config to return ``{"state": {"backend": value}}``."""

    def fake_load_config():
        return {"state": {"backend": value}} if value is not None else {}

    monkeypatch.setattr(
        "hermes_cli.config.load_config",
        fake_load_config,
    )


def test_default_backend_is_python(tmp_path: Path) -> None:
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        assert isinstance(db, SessionDB)
    finally:
        db.close()
    snapshot = factory.state_backend_diagnostics()
    assert snapshot["backend"] == "python"
    assert snapshot["requested"] == "python"
    assert snapshot["source"] == "default"
    assert snapshot["fallback_reason"] is None


def test_explicit_arg_overrides_env(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv(factory.ENV_VAR, "rust")
    db = factory.get_session_db(tmp_path / "state.db", backend="python")
    try:
        assert isinstance(db, SessionDB)
    finally:
        db.close()
    snapshot = factory.state_backend_diagnostics()
    assert snapshot["source"] == "arg"
    assert snapshot["backend"] == "python"


def test_env_var_selects_python(tmp_path: Path, monkeypatch) -> None:
    monkeypatch.setenv(factory.ENV_VAR, "python")
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        assert isinstance(db, SessionDB)
    finally:
        db.close()
    assert factory.state_backend_diagnostics()["source"] == "env"


def test_invalid_backend_raises(tmp_path: Path) -> None:
    with pytest.raises(factory.StateBackendError, match="Unsupported"):
        factory.get_session_db(tmp_path / "state.db", backend="foobar")


def test_config_backend_python(tmp_path: Path, monkeypatch) -> None:
    _stub_load_config(monkeypatch, "python")
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        assert isinstance(db, SessionDB)
    finally:
        db.close()
    assert factory.state_backend_diagnostics()["source"] == "config"


def test_explicit_rust_request_failure_raises(monkeypatch, tmp_path: Path) -> None:
    """When the user explicitly asks for Rust via env, failures should raise."""
    monkeypatch.setenv(factory.ENV_VAR, "rust")

    def boom(_db_path):  # noqa: ARG001
        raise RuntimeError("synthetic build failure")

    monkeypatch.setattr(factory, "_build_rust", boom)
    with pytest.raises(factory.StateBackendError, match="explicitly requested"):
        factory.get_session_db(tmp_path / "state.db")


def test_config_rust_failure_falls_back(monkeypatch, tmp_path: Path) -> None:
    """Config-driven Rust selection that fails should fall back to Python."""
    _stub_load_config(monkeypatch, "rust")

    def boom(_db_path):  # noqa: ARG001
        raise RuntimeError("synthetic build failure")

    monkeypatch.setattr(factory, "_build_rust", boom)
    db = factory.get_session_db(tmp_path / "state.db")
    try:
        assert isinstance(db, SessionDB)
    finally:
        db.close()
    snapshot = factory.state_backend_diagnostics()
    assert snapshot["backend"] == "python"
    assert snapshot["requested"] == "rust"
    assert snapshot["source"] == "config"
    assert "synthetic build failure" in snapshot["fallback_reason"]


def test_selection_logged_once(monkeypatch, tmp_path: Path, caplog) -> None:
    caplog.set_level("INFO", logger="hermes_state_factory")
    db1 = factory.get_session_db(tmp_path / "a.db")
    db1.close()
    db2 = factory.get_session_db(tmp_path / "b.db")
    db2.close()
    selection_log_lines = [
        r for r in caplog.records if "state backend" in r.getMessage()
    ]
    assert len(selection_log_lines) == 1


def test_diagnostics_unset_before_first_call() -> None:
    factory.reset_selection_for_tests()
    snapshot = factory.state_backend_diagnostics()
    assert snapshot == {
        "backend": None,
        "requested": None,
        "source": None,
        "db_path": None,
        "fallback_reason": None,
    }

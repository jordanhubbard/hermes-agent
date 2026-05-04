"""Run the existing SessionDB behavior suite against the Rust adapter.

This module deliberately reuses test classes from ``tests/test_hermes_state.py``
instead of maintaining a second hand-written parity suite. Tests that do not
exercise the ``db`` fixture are Python implementation unit tests, not backend
behavior tests, so they are skipped here.
"""

from __future__ import annotations

import importlib.util
import inspect
from pathlib import Path

import pytest

from hermes_state_rust import RustSessionDB


_SOURCE_PATH = Path(__file__).resolve().parents[1] / "test_hermes_state.py"
_SPEC = importlib.util.spec_from_file_location("_hermes_state_python_tests", _SOURCE_PATH)
_SOURCE = importlib.util.module_from_spec(_SPEC)
assert _SPEC.loader is not None
_SPEC.loader.exec_module(_SOURCE)


@pytest.fixture()
def db(tmp_path):
    session_db = RustSessionDB(tmp_path / "test_state.db")
    try:
        yield session_db
    finally:
        session_db.close()


_PYTHON_IMPLEMENTATION_TESTS = {
    "test_sqlite_timeout_is_at_least_30s",
}


def _export_backend_behavior_tests() -> None:
    for name, value in vars(_SOURCE).items():
        if not name.startswith("Test") or not isinstance(value, type):
            continue
        globals()[name] = _mark_python_only_methods(value)


def _mark_python_only_methods(cls):
    for name, value in vars(cls).items():
        if not name.startswith("test_") or not callable(value):
            continue
        params = inspect.signature(value).parameters
        if "db" not in params or name in _PYTHON_IMPLEMENTATION_TESTS:
            setattr(
                cls,
                name,
                pytest.mark.skip(reason="Python SessionDB implementation test")(
                    value
                ),
            )
    return cls


_export_backend_behavior_tests()

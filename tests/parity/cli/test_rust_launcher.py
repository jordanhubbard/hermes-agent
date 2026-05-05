"""Rust launcher smoke tests for the full-parity runtime selector."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CLI_CRATE = REPO_ROOT / "crates" / "hermes-cli"

pytestmark = pytest.mark.skipif(
    not RUST_CLI_CRATE.exists() or shutil.which("cargo") is None,
    reason="Rust launcher requires cargo; tracked by hermes-fpr.2",
)


def _run_launcher(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    return subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-cli",
            "--bin",
            "hermes",
            "--",
            *args,
        ],
        cwd=REPO_ROOT,
        env=merged_env,
        capture_output=True,
        text=True,
        timeout=120,
    )


def test_runtime_info_reports_default_python_rollout_path() -> None:
    result = _run_launcher("--runtime-info")

    assert result.returncode == 0, result.stderr
    info = json.loads(result.stdout)
    assert info["selected_runtime"] == "python"
    assert info["default_runtime"] == "python"
    assert info["selector_env"] == "HERMES_RUNTIME"


def test_runtime_info_reports_explicit_rust_selection() -> None:
    result = _run_launcher("--runtime-info", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["selected_runtime"] == "rust"


def test_rust_runtime_has_native_smoke_command() -> None:
    result = _run_launcher("version", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 0, result.stderr
    assert result.stdout.startswith("hermes rust launcher ")


def test_rust_runtime_rejects_unported_commands_without_python_import() -> None:
    result = _run_launcher("gateway", "status", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 78
    assert "not Rust-owned yet" in result.stderr
    assert "HERMES_RUNTIME=python" in result.stderr


def test_python_runtime_execs_explicit_fallback(tmp_path: Path) -> None:
    fake_python = tmp_path / "python"
    fake_python.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
    fake_python.chmod(fake_python.stat().st_mode | stat.S_IXUSR)

    result = _run_launcher(
        "version",
        env={"HERMES_RUNTIME": "python", "HERMES_PYTHON": str(fake_python)},
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.splitlines() == ["-m", "hermes_cli.main", "version"]


def test_install_script_builds_and_links_rust_launcher() -> None:
    script = (REPO_ROOT / "scripts" / "install.sh").read_text()

    assert "cargo build --release -p hermes-cli --bin hermes" in script
    assert 'RUST_HERMES_BIN="$INSTALL_DIR/target/release/hermes"' in script
    assert 'HERMES_BIN="$RUST_HERMES_BIN"' in script
    assert "HERMES_RUNTIME=python" in script


def test_update_flow_rebuilds_rust_launcher() -> None:
    main_py = (REPO_ROOT / "hermes_cli" / "main.py").read_text()

    assert "def _build_rust_hermes_launcher()" in main_py
    assert '"build", "--release", "-p", "hermes-cli", "--bin", "hermes"' in main_py
    assert "_build_rust_hermes_launcher()" in main_py
    assert "def _relink_rust_hermes_launcher" in main_py

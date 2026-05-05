"""Literal Hermes CLI smoke for the Rust state daemon backend.

Bead ``hermes-6eg``. This complements
``test_e2e_factory_daemon.py`` by invoking the installed ``hermes``
console script rather than calling the factory directly.
"""

from __future__ import annotations

import os
import shutil
import signal
import shlex
import stat
import subprocess
import sys
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[3]


pytestmark = pytest.mark.skipif(
    os.name != "posix" or shutil.which("cargo") is None,
    reason="Rust state daemon CLI smoke requires cargo and Unix sockets",
)


def _hermes_bin() -> Path:
    name = "hermes.exe" if os.name == "nt" else "hermes"
    candidates = [
        Path(sys.executable).parent / name,
        Path(sys.prefix) / "bin" / name,
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    pytest.skip(
        "hermes console script not found at "
        + ", ".join(str(candidate) for candidate in candidates)
    )


def _build_daemon() -> Path:
    result = subprocess.run(
        [
            "cargo",
            "build",
            "--quiet",
            "-p",
            "hermes-state",
            "--bin",
            "hermes_state_daemon",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"failed to build hermes_state_daemon: "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    daemon = REPO_ROOT / "target" / "debug" / "hermes_state_daemon"
    assert daemon.exists(), f"expected daemon binary at {daemon}"
    return daemon


def _daemon_wrapper(tmp_path: Path, daemon: Path) -> tuple[Path, Path]:
    marker = tmp_path / "daemon-invoked"
    pid_file = tmp_path / "daemon.pid"
    wrapper = tmp_path / "hermes_state_daemon_wrapper.sh"
    wrapper.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        f"printf '1\\n' > {shlex.quote(str(marker))}\n"
        f"printf '%s\\n' \"$$\" > {shlex.quote(str(pid_file))}\n"
        f"exec {shlex.quote(str(daemon))} \"$@\"\n"
    )
    wrapper.chmod(wrapper.stat().st_mode | stat.S_IXUSR)
    return wrapper, pid_file


def _cleanup_daemon(pid_file: Path) -> None:
    if not pid_file.exists():
        return
    try:
        pid = int(pid_file.read_text().strip())
    except (OSError, ValueError):
        return
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass


def test_literal_hermes_sessions_list_uses_rust_daemon(tmp_path: Path) -> None:
    daemon = _build_daemon()
    wrapper, pid_file = _daemon_wrapper(tmp_path, daemon)
    hermes_home = tmp_path / "hermes-home"

    env = os.environ.copy()
    env.update(
        {
            "HERMES_HOME": str(hermes_home),
            "HERMES_STATE_BACKEND": "rust",
            "HERMES_STATE_BOUNDARY": "daemon",
            "HERMES_STATE_DAEMON_BIN": str(wrapper),
        }
    )

    try:
        result = subprocess.run(
            [str(_hermes_bin()), "sessions", "list"],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            timeout=120,
        )
    finally:
        _cleanup_daemon(pid_file)

    assert result.returncode == 0, (
        f"hermes sessions list failed: "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "No sessions found." in result.stdout
    assert (tmp_path / "daemon-invoked").exists()
    assert (hermes_home / "state.db").exists()

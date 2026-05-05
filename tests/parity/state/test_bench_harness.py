"""Smoke test for scripts/bench_state.py.

Validates the harness runs end-to-end and produces the expected output
shape. Bead ``hermes-izz.4`` (objective perf data). Real benchmarks
are run from the command line — this test only proves the harness
itself doesn't bit-rot.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
BENCH_SCRIPT = REPO_ROOT / "scripts" / "bench_state.py"


def test_bench_harness_runs_python_only() -> None:
    """Even without cargo, the harness produces a python-backend row."""
    result = subprocess.run(
        [sys.executable, str(BENCH_SCRIPT), "--ops", "5", "--skip", "subprocess",
         "--skip", "daemon"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, (
        f"bench harness failed: stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "python" in result.stdout
    assert "ops_per_s" in result.stdout


@pytest.mark.skipif(shutil.which("cargo") is None, reason="cargo not installed")
def test_bench_harness_runs_with_daemon() -> None:
    """With cargo present, the harness runs the daemon backend too."""
    result = subprocess.run(
        [sys.executable, str(BENCH_SCRIPT), "--ops", "5", "--skip", "subprocess"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"bench harness failed: stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    assert "python" in result.stdout
    assert "daemon" in result.stdout
    assert "Ratios vs python" in result.stdout

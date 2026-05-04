"""Lint that docs/rust-parity/README.md matches docs/rust-parity/status.yaml.

If this fails, run:

    .venv/bin/python scripts/rust_parity_status.py --write

and commit the regenerated README.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def test_rust_parity_status_check() -> None:
    result = subprocess.run(
        [sys.executable, "scripts/rust_parity_status.py", "--check"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        "rust-parity status check failed:\n"
        f"stdout: {result.stdout}\nstderr: {result.stderr}"
    )

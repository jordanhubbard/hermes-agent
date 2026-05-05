"""Executable Python-vs-Rust shadow diff gate for full-parity rollout."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[2]
HARNESS = REPO_ROOT / "scripts" / "rust_shadow_diff.py"


pytestmark = pytest.mark.skipif(
    shutil.which("cargo") is None,
    reason="Rust shadow diff requires cargo; tracked by hermes-fpr.9",
)


def test_rust_shadow_diff_has_no_unexplained_divergences() -> None:
    result = subprocess.run(
        [
            sys.executable,
            str(HARNESS),
            "--json",
            "--fail-on-divergence",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=300,
    )

    assert result.returncode == 0, result.stdout + result.stderr
    payload = json.loads(result.stdout)
    assert payload["protocol"] == "hermes.shadow_diff.v1"
    assert payload["summary"]["divergences"] == 0
    assert payload["summary"]["mutable_cases"] >= 1
    assert {
        "prompts_tool_calls",
        "cli_commands",
        "gateway_events",
        "state_mutations",
    } <= set(payload["summary"]["surfaces"])

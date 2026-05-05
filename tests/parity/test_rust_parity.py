"""Replay parity fixtures through the Rust agent core."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

from tests.parity.fixture_schema import (
    ReplayResult,
    assert_replay_matches_expected,
    iter_fixtures,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
RUST_AGENT_CORE_CRATE = REPO_ROOT / "crates" / "hermes-agent-core"
FIXTURES = list(iter_fixtures())


pytestmark = pytest.mark.skipif(
    not RUST_AGENT_CORE_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-agent-core not yet built; tracked by hermes-1oa",
)


@pytest.mark.parametrize(
    ("fixture_path", "fixture"),
    FIXTURES,
    ids=[path.stem for path, _fixture in FIXTURES],
)
def test_rust_loader_replays_fixture(fixture_path: Path, fixture: dict) -> None:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-agent-core",
            "--bin",
            "hermes_agent_replay",
            "--",
            str(fixture_path),
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust replay failed for {fixture_path}: "
        f"stdout={result.stdout!r} stderr={result.stderr!r}"
    )
    payload = json.loads(result.stdout)
    replay_result = ReplayResult(**payload)
    assert_replay_matches_expected(
        fixture["expected"], replay_result, source=str(fixture_path)
    )

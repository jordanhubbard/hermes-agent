"""Skeleton for replaying parity fixtures through the Rust agent core.

This test is intentionally skipped until ``crates/hermes-agent-core`` exists
(tracked by bead hermes-1oa). When that crate lands, the body should:

  1. Spawn the Rust replay binary (or call into a PyO3 module) for each
     fixture in tests/parity/fixtures/.
  2. Pass the fixture JSON to the Rust side.
  3. Receive a ReplayResult-shaped payload.
  4. Compare against the same ``expected`` block as test_python_parity.py
     using ``assert_replay_matches_expected``.

The intent is that the same fixtures gate both backends — that is the entire
point of bead hermes-ni1.2.
"""

from __future__ import annotations

import shutil
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]
RUST_AGENT_CORE_CRATE = REPO_ROOT / "crates" / "hermes-agent-core"


pytestmark = pytest.mark.skipif(
    not RUST_AGENT_CORE_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-agent-core not yet built; tracked by hermes-1oa",
)


def test_rust_loader_replays_all_fixtures() -> None:
    pytest.skip(
        "Rust loader not implemented. Activate when hermes-1oa.1 and 1oa.2 land."
    )

"""Structural lint for docs/rust-parity/cutover.md.

Bead hermes-ni1.4 requires the doc to define parity gates, performance
gates, data compatibility, rollout flags, rollback steps, known
deferrals, owner sign-off, and post-cutover monitoring. If any of those
sections is missing or renamed, this test fails so the doc skeleton
cannot silently regress.
"""

from __future__ import annotations

import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CUTOVER_MD = REPO_ROOT / "docs" / "rust-parity" / "cutover.md"

REQUIRED_HEADINGS = (
    "Parity gates",
    "Performance gates",
    "Data compatibility requirements",
    "Rollout flags",
    "Rollback steps",
    "Known deferrals",
    "Owner sign-off",
    "Post-cutover monitoring",
)


def test_cutover_doc_exists() -> None:
    assert CUTOVER_MD.exists(), f"missing cutover doc at {CUTOVER_MD}"


def test_cutover_doc_has_required_sections() -> None:
    text = CUTOVER_MD.read_text()
    headings = set(re.findall(r"^##+\s+(.+?)\s*$", text, flags=re.MULTILINE))
    missing = [h for h in REQUIRED_HEADINGS if h not in headings]
    assert not missing, (
        f"cutover.md missing required sections: {missing}\n"
        f"present: {sorted(headings)}"
    )

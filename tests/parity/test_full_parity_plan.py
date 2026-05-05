"""Structural lint for the full Rust parity / Python removal plan."""

from __future__ import annotations

from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_MD = REPO_ROOT / "docs" / "rust-parity" / "full-parity-plan.md"
STATUS_YAML = REPO_ROOT / "docs" / "rust-parity" / "status.yaml"

REQUIRED_SECTIONS = (
    "Definition of Done",
    "Workstreams",
    "Cutover Rules",
    "Non-Goals",
)


def test_full_parity_plan_exists_and_has_required_sections() -> None:
    text = PLAN_MD.read_text()
    for section in REQUIRED_SECTIONS:
        assert f"## {section}" in text


def test_full_parity_epic_tracks_python_removal_gate() -> None:
    data = yaml.safe_load(STATUS_YAML.read_text())
    epics = {epic["id"]: epic for epic in data["epics"]}
    assert "hermes-fpr" in epics

    rows = epics["hermes-fpr"]["rows"]
    assert len(rows) >= 10
    assert rows[-1]["bead"] == "hermes-fpr.10"
    assert "remove Python sources" in rows[-1]["story"]
    assert rows[-1]["status"] == "planned"

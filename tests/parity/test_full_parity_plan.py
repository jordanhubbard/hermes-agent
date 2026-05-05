"""Structural lint for the full Rust parity / Python removal plan."""

from __future__ import annotations

from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_MD = REPO_ROOT / "docs" / "rust-parity" / "full-parity-plan.md"
AUDIT_MD = REPO_ROOT / "docs" / "rust-parity" / "entrypoint-audit.md"
STATUS_YAML = REPO_ROOT / "docs" / "rust-parity" / "status.yaml"

REQUIRED_SECTIONS = (
    "Definition of Done",
    "Workstreams",
    "Cutover Rules",
    "Non-Goals",
)

REQUIRED_AUDIT_SURFACES = (
    "CLI",
    "Agent",
    "Gateway",
    "TUI",
    "Dashboard",
    "ACP",
    "Cron",
    "Batch",
    "MCP",
    "Tools",
    "Skills",
    "Plugins",
)


def test_full_parity_plan_exists_and_has_required_sections() -> None:
    text = PLAN_MD.read_text()
    for section in REQUIRED_SECTIONS:
        assert f"## {section}" in text
    assert "docs/rust-parity/entrypoint-audit.md" in text


def test_entrypoint_audit_covers_required_surfaces() -> None:
    text = AUDIT_MD.read_text()
    assert "## Entry Point Inventory" in text
    assert "## Coverage Checklist" in text
    assert "no installed user-facing Hermes command is Rust-primary" in text
    assert 'hermes = "hermes_cli.main:main"' in text
    assert 'hermes-agent = "run_agent:main"' in text
    assert 'hermes-acp = "acp_adapter.entry:main"' in text
    for surface in REQUIRED_AUDIT_SURFACES:
        assert f"| {surface} |" in text


def test_full_parity_epic_tracks_python_removal_gate() -> None:
    data = yaml.safe_load(STATUS_YAML.read_text())
    epics = {epic["id"]: epic for epic in data["epics"]}
    assert "hermes-fpr" in epics

    rows = epics["hermes-fpr"]["rows"]
    assert len(rows) >= 10
    assert rows[0]["bead"] == "hermes-fpr.1"
    assert rows[0]["status"] == "tested"
    assert "entrypoint-audit.md" in rows[0]["rust_target"]
    assert rows[-1]["bead"] == "hermes-fpr.10"
    assert "remove Python sources" in rows[-1]["story"]
    assert rows[-1]["status"] == "planned"

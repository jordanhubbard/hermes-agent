"""Rust/Python parity for gateway slash and control command routing."""

from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_GATEWAY_CRATE = REPO_ROOT / "crates" / "hermes-gateway"

SAMPLES = (
    "/approve always",
    "/deny",
    "/yolo",
    "/reload-mcp",
    "/reload-skills",
    "/title Project Atlas",
    "/resume sprint-notes",
    "/background run report",
    "/bg run report",
    "/queue next turn",
    "/steer after tool",
    "/status",
    "/help",
    "/verbose",
    "/tools list",
    "/unknown",
)


pytestmark = pytest.mark.skipif(
    not RUST_GATEWAY_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-gateway slash routing not yet built; tracked by hermes-4ne.2",
)


def _rust_routes() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-gateway",
            "--bin",
            "hermes_gateway_adapter_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust gateway snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)["gateway_command_routes"]


def _expected_action(canonical: str, args: str) -> str:
    if canonical == "approve":
        return "approval_allow_always" if args.strip() == "always" else "approval_allow_once"
    return {
        "deny": "approval_deny",
        "yolo": "toggle_yolo",
        "reload-mcp": "reload_mcp",
        "reload-skills": "reload_skills_deferred",
        "title": "set_title",
        "resume": "resume_session",
        "background": "background_prompt" if args.strip() else "usage_error",
        "queue": "queue_prompt" if args.strip() else "usage_error",
        "steer": "steer_prompt" if args.strip() else "usage_error",
        "status": "status",
        "help": "help",
        "commands": "help",
        "new": "new_session",
        "stop": "stop_or_interrupt",
        "profile": "profile",
        "agents": "agents_status",
        "restart": "restart_gateway",
        "update": "update",
    }.get(canonical, "dispatch_or_busy")


def _python_routes() -> dict:
    from hermes_cli.commands import GATEWAY_KNOWN_COMMANDS, resolve_command, should_bypass_active_session

    routes = {}
    for sample in SAMPLES:
        stripped = sample.strip()
        command_text = stripped[1:]
        command_name = command_text.split(None, 1)[0].lower()
        args = command_text[len(command_name):].lstrip()
        cmd = resolve_command(command_name)
        if cmd is None:
            routes[sample] = None
            continue
        known = cmd.name in GATEWAY_KNOWN_COMMANDS or any(alias in GATEWAY_KNOWN_COMMANDS for alias in cmd.aliases)
        if not known:
            routes[sample] = None
            continue
        routes[sample] = {
            "raw": stripped,
            "canonical_name": cmd.name,
            "args": args,
            "known_to_gateway": True,
            "bypass_active_session": should_bypass_active_session(cmd.name),
            "action": _expected_action(cmd.name, args),
        }
    return routes


def test_rust_gateway_slash_routes_match_python_registry() -> None:
    assert _rust_routes() == _python_routes()


def test_rust_gateway_control_routes_cover_representative_flows() -> None:
    routes = _rust_routes()
    assert routes["/approve always"]["action"] == "approval_allow_always"
    assert routes["/deny"]["action"] == "approval_deny"
    assert routes["/yolo"]["action"] == "toggle_yolo"
    assert routes["/reload-mcp"]["action"] == "reload_mcp"
    assert routes["/title Project Atlas"]["args"] == "Project Atlas"
    assert routes["/resume sprint-notes"]["action"] == "resume_session"
    assert routes["/bg run report"]["canonical_name"] == "background"
    assert routes["/status"]["bypass_active_session"] is True
    assert routes["/tools list"] is None

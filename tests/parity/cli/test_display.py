"""Rust/Python parity for stable CLI display, skin, log, and status surfaces."""

from __future__ import annotations

import json
import shutil
import subprocess
from datetime import datetime
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CLI_CRATE = REPO_ROOT / "crates" / "hermes-cli"


pytestmark = pytest.mark.skipif(
    not RUST_CLI_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-cli display surfaces not yet built; tracked by hermes-3n2.4",
)


def _rust_snapshot() -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-cli",
            "--bin",
            "hermes_cli_display_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust display snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot() -> dict:
    from cli import HermesCLI
    from hermes_cli.skin_engine import _BUILTIN_SKINS

    cli_obj = HermesCLI.__new__(HermesCLI)
    cli_obj.console = MagicMock()
    cli_obj.agent = SimpleNamespace(session_total_tokens=321, session_api_calls=4)
    cli_obj.session_id = "session-123"
    cli_obj.session_start = datetime(2026, 4, 9, 19, 24)
    cli_obj.model = "openai/gpt-5.4"
    cli_obj.provider = "openai"
    cli_obj._agent_running = False
    cli_obj._session_db = MagicMock()
    cli_obj._session_db.get_session.return_value = {
        "title": "My titled session",
        "started_at": 1775791440,
        "updated_at": 1775791500,
    }

    with patch("cli.display_hermes_home", return_value="~/.hermes"):
        cli_obj._show_session_status()
    status = str(cli_obj.console.print.call_args.args[0])

    skins = []
    for name, data in _BUILTIN_SKINS.items():
        colors = data.get("colors", {})
        branding = data.get("branding", {})
        spinner = data.get("spinner", {})
        skins.append(
            {
                "name": name,
                "description": data.get("description", ""),
                "tool_prefix": data.get("tool_prefix", "┊"),
                "banner_title": colors.get("banner_title", ""),
                "response_border": colors.get("response_border", ""),
                "status_bar_bg": colors.get("status_bar_bg", ""),
                "agent_name": branding.get("agent_name", ""),
                "response_label": branding.get("response_label", ""),
                "prompt_symbol": branding.get("prompt_symbol", ""),
                "help_header": branding.get("help_header", ""),
                "spinner_wing_count": len(spinner.get("wings", [])),
            }
        )

    log_dir = "/tmp/hermes/logs"
    return {
        "skins": skins,
        "status": status,
        "logging_cli": {
            "log_dir": log_dir,
            "agent_log": f"{log_dir}/agent.log",
            "errors_log": f"{log_dir}/errors.log",
            "gateway_log": None,
            "agent_level": "INFO",
            "agent_max_bytes": 5 * 1024 * 1024,
            "agent_backup_count": 3,
            "errors_max_bytes": 2 * 1024 * 1024,
            "errors_backup_count": 2,
            "gateway_component_prefixes": ["gateway"],
        },
        "logging_gateway": {
            "log_dir": log_dir,
            "agent_log": f"{log_dir}/agent.log",
            "errors_log": f"{log_dir}/errors.log",
            "gateway_log": f"{log_dir}/gateway.log",
            "agent_level": "INFO",
            "agent_max_bytes": 5 * 1024 * 1024,
            "agent_backup_count": 3,
            "errors_max_bytes": 2 * 1024 * 1024,
            "errors_backup_count": 2,
            "gateway_component_prefixes": ["gateway"],
        },
        "contains_erase_to_eol": any(
            marker in status or any(marker in str(skin) for skin in skins)
            for marker in ("\x1b[K", "\\033[K")
        ),
    }


def test_rust_display_snapshot_matches_python() -> None:
    assert _rust_snapshot() == _python_snapshot()


def test_display_sources_do_not_reintroduce_ansi_erase_to_eol() -> None:
    for rel in ("agent/display.py", "cli.py", "hermes_cli/skin_engine.py"):
        text = (REPO_ROOT / rel).read_text(errors="ignore")
        assert "\\033[K" not in text
        assert "\x1b[K" not in text

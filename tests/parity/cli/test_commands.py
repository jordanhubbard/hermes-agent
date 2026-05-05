"""Rust/Python parity for the CLI command registry and slash dispatch."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CLI_CRATE = REPO_ROOT / "crates" / "hermes-cli"


pytestmark = pytest.mark.skipif(
    not RUST_CLI_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-cli not yet built; tracked by hermes-3n2.1",
)


def _rust_snapshot(*, gated: bool = False) -> dict:
    env = os.environ.copy()
    env["HERMES_CLI_CONFIG_GATES"] = "verbose" if gated else ""
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-cli",
            "--bin",
            "hermes_cli_registry",
        ],
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust CLI registry failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot(monkeypatch: pytest.MonkeyPatch, tmp_path: Path, *, gated: bool = False) -> dict:
    hermes_home = tmp_path / "hermes"
    hermes_home.mkdir()
    if gated:
        (hermes_home / "config.yaml").write_text("display:\n  tool_progress_command: true\n")
    monkeypatch.setenv("HERMES_HOME", str(hermes_home))

    from hermes_cli.commands import (
        COMMANDS,
        COMMANDS_BY_CATEGORY,
        COMMAND_REGISTRY,
        GATEWAY_KNOWN_COMMANDS,
        SUBCOMMANDS,
        gateway_help_lines,
        slack_subcommand_map,
        telegram_bot_commands,
    )

    def dispatch_sample(text: str) -> dict | None:
        from hermes_cli.commands import resolve_command

        stripped = text.strip()
        if not stripped.startswith("/"):
            return None
        command_text = stripped[1:]
        command_name = command_text.split(None, 1)[0].lower()
        if not command_name:
            return None
        cmd = resolve_command(command_name)
        if cmd is None:
            return None
        args = command_text[len(command_name):].lstrip()
        return {
            "original": stripped,
            "command_name": command_name,
            "canonical_name": cmd.name,
            "args": args,
            "is_gateway_known": command_name in GATEWAY_KNOWN_COMMANDS
            or cmd.name in GATEWAY_KNOWN_COMMANDS
            or any(alias in GATEWAY_KNOWN_COMMANDS for alias in cmd.aliases),
        }

    return {
        "registry": [
            {
                "name": cmd.name,
                "description": cmd.description,
                "category": cmd.category,
                "aliases": list(cmd.aliases),
                "args_hint": cmd.args_hint,
                "subcommands": list(cmd.subcommands),
                "cli_only": cmd.cli_only,
                "gateway_only": cmd.gateway_only,
                "gateway_config_gate": cmd.gateway_config_gate,
            }
            for cmd in COMMAND_REGISTRY
        ],
        "commands": dict(sorted(COMMANDS.items())),
        "commands_by_category": {
            category: dict(sorted(commands.items()))
            for category, commands in sorted(COMMANDS_BY_CATEGORY.items())
        },
        "subcommands": {key: list(value) for key, value in sorted(SUBCOMMANDS.items())},
        "gateway_known_commands": sorted(GATEWAY_KNOWN_COMMANDS),
        "gateway_help_lines": gateway_help_lines(),
        "telegram_bot_commands": [list(item) for item in telegram_bot_commands()],
        "slack_subcommand_map": dict(sorted(slack_subcommand_map().items())),
        "dispatch_samples": {
            sample: dispatch_sample(sample)
            for sample in (
                "/bg ship it",
                "/reset",
                "/reload_mcp",
                "/clear",
                "/help",
                "/unknown",
                "plain user message",
            )
        },
    }


def test_rust_command_registry_matches_python_default(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    assert _rust_snapshot() == _python_snapshot(monkeypatch, tmp_path)


def test_rust_command_registry_matches_python_config_gated_gateway_surfaces(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    rust = _rust_snapshot(gated=True)
    python = _python_snapshot(monkeypatch, tmp_path, gated=True)

    assert rust["gateway_help_lines"] == python["gateway_help_lines"]
    assert rust["telegram_bot_commands"] == python["telegram_bot_commands"]
    assert rust["slack_subcommand_map"] == python["slack_subcommand_map"]
    assert any(line.startswith("`/verbose`") for line in rust["gateway_help_lines"])

#!/usr/bin/env python3
"""Shadow-diff representative Python and Rust Hermes behavior.

This is the executable gate for the hermes-fpr.9 cutover bead. It compares
covered Python and Rust paths on the same inputs and exits non-zero when an
unclassified divergence appears. It intentionally covers representative
surfaces rather than pretending the Rust runtime is deletion-ready.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterator


REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))
GATEWAY_SAMPLES = (
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


@dataclass
class ShadowCase:
    name: str
    surface: str
    mutable: bool
    status: str
    classification: str | None = None
    python: Any | None = None
    rust: Any | None = None
    error: str | None = None


@contextmanager
def isolated_env(**updates: str) -> Iterator[None]:
    old: dict[str, str | None] = {key: os.environ.get(key) for key in updates}
    os.environ.update(updates)
    try:
        yield
    finally:
        for key, value in old.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def _json(obj: Any) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), default=str)


def _cargo_json(package: str, binary: str, *args: str, timeout: int = 180) -> Any:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            package,
            "--bin",
            binary,
            "--",
            *args,
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"{package}/{binary} failed with {result.returncode}: "
            f"stdout={result.stdout!r} stderr={result.stderr!r}"
        )
    return json.loads(result.stdout)


def _compare(
    cases: list[ShadowCase],
    *,
    name: str,
    surface: str,
    mutable: bool,
    python: Any,
    rust: Any,
) -> None:
    if _json(python) == _json(rust):
        cases.append(
            ShadowCase(
                name=name,
                surface=surface,
                mutable=mutable,
                status="matched",
                python=python,
                rust=rust,
            )
        )
        return
    cases.append(
        ShadowCase(
            name=name,
            surface=surface,
            mutable=mutable,
            status="diverged",
            classification="unexplained",
            python=python,
            rust=rust,
        )
    )


def _python_agent_replay(fixture_path: Path) -> dict[str, Any]:
    from tests.parity.fixture_schema import load_fixture, replay

    return asdict(replay(load_fixture(fixture_path)))


def _agent_replay_cases(cases: list[ShadowCase]) -> None:
    from tests.parity.fixture_schema import FIXTURE_DIR

    for fixture_path in sorted(FIXTURE_DIR.glob("*.json")):
        python = _python_agent_replay(fixture_path)
        rust = _cargo_json(
            "hermes-agent-core",
            "hermes_agent_replay",
            str(fixture_path),
        )
        _compare(
            cases,
            name=f"agent_replay:{fixture_path.stem}",
            surface="prompts_tool_calls",
            mutable=False,
            python=python,
            rust=rust,
        )


def _dispatch_sample(text: str) -> dict[str, Any] | None:
    from hermes_cli.commands import GATEWAY_KNOWN_COMMANDS, resolve_command

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
    args = command_text[len(command_name) :].lstrip()
    return {
        "original": stripped,
        "command_name": command_name,
        "canonical_name": cmd.name,
        "args": args,
        "is_gateway_known": command_name in GATEWAY_KNOWN_COMMANDS
        or cmd.name in GATEWAY_KNOWN_COMMANDS
        or any(alias in GATEWAY_KNOWN_COMMANDS for alias in cmd.aliases),
    }


def _python_cli_registry_snapshot(hermes_home: Path) -> dict[str, Any]:
    with isolated_env(HERMES_HOME=str(hermes_home)):
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
            "subcommands": {
                key: list(value) for key, value in sorted(SUBCOMMANDS.items())
            },
            "gateway_known_commands": sorted(GATEWAY_KNOWN_COMMANDS),
            "gateway_help_lines": gateway_help_lines(),
            "telegram_bot_commands": [
                list(item) for item in telegram_bot_commands()
            ],
            "slack_subcommand_map": dict(sorted(slack_subcommand_map().items())),
            "dispatch_samples": {
                sample: _dispatch_sample(sample)
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


def _cli_cases(cases: list[ShadowCase], work_root: Path) -> None:
    hermes_home = work_root / "cli-hermes-home"
    hermes_home.mkdir(parents=True, exist_ok=True)
    rust = _cargo_json("hermes-cli", "hermes_cli_registry")
    python = _python_cli_registry_snapshot(hermes_home)
    _compare(
        cases,
        name="cli_registry:default",
        surface="cli_commands",
        mutable=False,
        python=python,
        rust=rust,
    )


def _expected_gateway_action(canonical: str, args: str) -> str:
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


def _python_gateway_routes() -> dict[str, Any]:
    from hermes_cli.commands import (
        GATEWAY_KNOWN_COMMANDS,
        resolve_command,
        should_bypass_active_session,
    )

    routes: dict[str, Any] = {}
    for sample in GATEWAY_SAMPLES:
        stripped = sample.strip()
        command_text = stripped[1:]
        command_name = command_text.split(None, 1)[0].lower()
        args = command_text[len(command_name) :].lstrip()
        cmd = resolve_command(command_name)
        if cmd is None:
            routes[sample] = None
            continue
        known = cmd.name in GATEWAY_KNOWN_COMMANDS or any(
            alias in GATEWAY_KNOWN_COMMANDS for alias in cmd.aliases
        )
        if not known:
            routes[sample] = None
            continue
        routes[sample] = {
            "raw": stripped,
            "canonical_name": cmd.name,
            "args": args,
            "known_to_gateway": True,
            "bypass_active_session": should_bypass_active_session(cmd.name),
            "action": _expected_gateway_action(cmd.name, args),
        }
    return routes


def _gateway_cases(cases: list[ShadowCase]) -> None:
    rust = _cargo_json("hermes-gateway", "hermes_gateway_adapter_snapshot")
    _compare(
        cases,
        name="gateway_routes:control_commands",
        surface="gateway_events",
        mutable=False,
        python=_python_gateway_routes(),
        rust=rust["gateway_command_routes"],
    )


def _project_state(db: Any) -> dict[str, Any]:
    session_id = db.create_session(
        "shadow-1",
        source="cli",
        model="gpt-4o",
        system_prompt="you are hermes",
    )
    db.append_message(session_id, role="user", content="Read README.md")
    db.append_message(
        session_id,
        role="assistant",
        tool_calls=[
            {
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": '{"path": "README.md"}',
                },
            }
        ],
    )
    db.append_message(
        session_id,
        role="tool",
        content="Hermes Agent\nA local-first AI agent.",
        tool_name="read_file",
        tool_call_id="call_1",
    )
    db.append_message(
        session_id,
        role="assistant",
        content="The README begins with 'Hermes Agent'.",
        reasoning="The first line of the file is 'Hermes Agent'.",
    )
    db.update_token_counts(
        session_id,
        input_tokens=42,
        output_tokens=17,
        cache_read_tokens=0,
        cache_write_tokens=0,
        reasoning_tokens=8,
    )

    messages = db.get_messages(session_id)
    matches = db.search_messages("Hermes")
    rich = db.list_sessions_rich(limit=10)
    db.end_session(session_id, end_reason="completed")
    finalized = db.get_session(session_id)
    deleted = db.delete_session(session_id)
    after_delete = db.get_session(session_id)

    return {
        "session_id": session_id,
        "messages": [
            {
                "role": message.get("role"),
                "has_content": bool(message.get("content")),
                "tool_name": message.get("tool_name"),
                "tool_call_id": message.get("tool_call_id"),
                "has_tool_calls": bool(message.get("tool_calls")),
                "has_reasoning": bool(message.get("reasoning")),
            }
            for message in messages
        ],
        "search_hit_sessions": sorted(
            {
                row.get("session_id")
                for row in matches
                if row.get("session_id") == session_id
            }
        ),
        "rich_session": _select_session_projection(rich, session_id),
        "finalized": _select_session_projection([finalized], session_id),
        "deleted": deleted,
        "after_delete_is_none": after_delete is None,
    }


def _select_session_projection(rows: list[dict[str, Any] | None], session_id: str) -> dict[str, Any]:
    row = next((item for item in rows if item and item.get("id") == session_id), None)
    if not row:
        return {}
    return {
        "id": row.get("id"),
        "source": row.get("source"),
        "model": row.get("model"),
        "end_reason": row.get("end_reason"),
        "message_count": row.get("message_count"),
        "tool_call_count": row.get("tool_call_count"),
        "input_tokens": row.get("input_tokens"),
        "output_tokens": row.get("output_tokens"),
        "reasoning_tokens": row.get("reasoning_tokens"),
    }


def _state_cases(cases: list[ShadowCase], work_root: Path) -> None:
    from hermes_state import SessionDB
    from hermes_state_rust import RustSessionDB

    python_db = SessionDB(work_root / "python-state.db")
    rust_db = RustSessionDB(work_root / "rust-state.db", boundary="daemon")
    try:
        python = _project_state(python_db)
        rust = _project_state(rust_db)
    finally:
        python_db.close()
        rust_db.close()
    _compare(
        cases,
        name="state_lifecycle:session_messages_search_delete",
        surface="state_mutations",
        mutable=True,
        python=python,
        rust=rust,
    )


def run_shadow(work_root: Path) -> dict[str, Any]:
    if shutil.which("cargo") is None:
        raise RuntimeError("cargo is required for Rust shadow diff")

    cases: list[ShadowCase] = []
    for fn in (_agent_replay_cases, _cli_cases, _gateway_cases, _state_cases):
        try:
            if fn in (_cli_cases, _state_cases):
                fn(cases, work_root)  # type: ignore[misc]
            else:
                fn(cases)  # type: ignore[misc]
        except Exception as exc:
            cases.append(
                ShadowCase(
                    name=fn.__name__,
                    surface="harness",
                    mutable=False,
                    status="error",
                    classification="unexplained",
                    error=str(exc),
                )
            )

    divergences = [
        case for case in cases if case.status in {"diverged", "error"}
    ]
    return {
        "protocol": "hermes.shadow_diff.v1",
        "summary": {
            "total_cases": len(cases),
            "matched": sum(1 for case in cases if case.status == "matched"),
            "divergences": len(divergences),
            "mutable_cases": sum(1 for case in cases if case.mutable),
            "surfaces": sorted({case.surface for case in cases}),
        },
        "cases": [asdict(case) for case in cases],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit JSON")
    parser.add_argument(
        "--fail-on-divergence",
        action="store_true",
        help="exit 1 if any unexplained divergence is found",
    )
    args = parser.parse_args(argv)

    with tempfile.TemporaryDirectory(prefix="hermes-shadow-") as tmp:
        with isolated_env(HERMES_HOME=str(Path(tmp) / "hermes-home")):
            result = run_shadow(Path(tmp))

    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        summary = result["summary"]
        print(
            "shadow diff: "
            f"{summary['matched']}/{summary['total_cases']} matched; "
            f"{summary['divergences']} divergences"
        )
        for case in result["cases"]:
            if case["status"] != "matched":
                print(
                    f"- {case['name']}: {case['status']} "
                    f"({case.get('classification') or 'unclassified'})"
                )

    if args.fail_on_divergence and result["summary"]["divergences"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

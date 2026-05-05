"""Rust/Python parity for non-chat integration boundaries."""

from __future__ import annotations

import ast
import json
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CRATE = REPO_ROOT / "crates" / "hermes-integrations"
CRON_JOBS = REPO_ROOT / "cron" / "jobs.py"
CRON_SCHEDULER = REPO_ROOT / "cron" / "scheduler.py"
BATCH_RUNNER = REPO_ROOT / "batch_runner.py"
MCP_SERVE = REPO_ROOT / "mcp_serve.py"
RL_CLI = REPO_ROOT / "rl_cli.py"
PLUGINS = REPO_ROOT / "hermes_cli" / "plugins.py"
PLUGINS_CMD = REPO_ROOT / "hermes_cli" / "plugins_cmd.py"
BOUNDARY_DOC = REPO_ROOT / "docs" / "rust-parity" / "integration-boundaries.md"

pytestmark = pytest.mark.skipif(
    not RUST_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-integrations not yet built; tracked by hermes-dwg.4",
)


def _rust_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-integrations",
            "--bin",
            "hermes_integrations_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust integrations snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _tree(path: Path) -> ast.Module:
    return ast.parse(path.read_text(encoding="utf-8"), filename=str(path))


def _top_level_function_names(path: Path) -> set[str]:
    return {
        node.name
        for node in _tree(path).body
        if isinstance(node, ast.FunctionDef) and not node.name.startswith("_")
    }


def _function_args(path: Path, name: str, *, class_name: str | None = None) -> list[str]:
    nodes: list[ast.stmt]
    if class_name is None:
        nodes = _tree(path).body
    else:
        klass = next(
            node
            for node in _tree(path).body
            if isinstance(node, ast.ClassDef) and node.name == class_name
        )
        nodes = klass.body
    fn = next(
        node
        for node in nodes
        if isinstance(node, ast.FunctionDef) and node.name == name
    )
    return [arg.arg for arg in fn.args.args if arg.arg != "self"]


def _assigned_literal(path: Path, name: str) -> Any:
    for node in _tree(path).body:
        if isinstance(node, ast.Assign):
            targets = [t.id for t in node.targets if isinstance(t, ast.Name)]
            if name in targets:
                return ast.literal_eval(node.value)
        if (
            isinstance(node, ast.AnnAssign)
            and isinstance(node.target, ast.Name)
            and node.target.id == name
        ):
            return ast.literal_eval(node.value)
    raise AssertionError(f"{name} not found in {path}")


def _frozenset_literal(path: Path, name: str) -> set[str]:
    for node in _tree(path).body:
        if not isinstance(node, ast.Assign):
            continue
        targets = [t.id for t in node.targets if isinstance(t, ast.Name)]
        if name not in targets:
            continue
        value = node.value
        if isinstance(value, ast.Call) and isinstance(value.func, ast.Name):
            assert value.func.id == "frozenset"
            return set(ast.literal_eval(value.args[0]))
    raise AssertionError(f"{name} not found in {path}")


def _dict_literal_keys(path: Path, name: str) -> set[str]:
    value = _assigned_literal(path, name)
    assert isinstance(value, dict)
    return set(value)


def _dict_literal_values(path: Path, name: str) -> set[str]:
    value = _assigned_literal(path, name)
    assert isinstance(value, dict)
    return set(value.values())


def _class_method_names(path: Path, name: str) -> list[str]:
    klass = next(
        node
        for node in _tree(path).body
        if isinstance(node, ast.ClassDef) and node.name == name
    )
    return [
        node.name
        for node in klass.body
        if isinstance(node, ast.FunctionDef) and not node.name.startswith("_")
    ]


def _dataclass_fields(path: Path, name: str) -> list[str]:
    klass = next(
        node
        for node in _tree(path).body
        if isinstance(node, ast.ClassDef) and node.name == name
    )
    fields: list[str] = []
    for node in klass.body:
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            fields.append(node.target.id)
    return fields


def _mcp_tool_names() -> list[str]:
    names: list[str] = []
    for node in ast.walk(_tree(MCP_SERVE)):
        if not isinstance(node, ast.FunctionDef):
            continue
        for deco in node.decorator_list:
            if (
                isinstance(deco, ast.Call)
                and isinstance(deco.func, ast.Attribute)
                and deco.func.attr == "tool"
                and isinstance(deco.func.value, ast.Name)
                and deco.func.value.id == "mcp"
            ):
                names.append(node.name)
    return names


def test_cron_boundary_matches_job_scheduler_contract() -> None:
    rust = _rust_snapshot()["cron"]

    job_funcs = _top_level_function_names(CRON_JOBS)
    scheduler_funcs = _top_level_function_names(CRON_SCHEDULER)

    assert set(rust["job_api"]) <= job_funcs
    assert set(rust["scheduler_api"]) <= scheduler_funcs
    assert set(rust["schedule_kinds"]) == {"once", "interval", "cron"}
    for kind in rust["schedule_kinds"]:
        assert f'"kind": "{kind}"' in CRON_JOBS.read_text(encoding="utf-8")

    assert set(rust["known_delivery_platforms"]) == _frozenset_literal(
        CRON_SCHEDULER,
        "_KNOWN_DELIVERY_PLATFORMS",
    )
    assert set(rust["home_target_env_vars"]) == _dict_literal_values(
        CRON_SCHEDULER,
        "_HOME_TARGET_ENV_VARS",
    )


def test_batch_boundary_matches_cli_runner_contract() -> None:
    rust = _rust_snapshot()["batch"]
    source = BATCH_RUNNER.read_text(encoding="utf-8")

    assert rust["cli_args"] == _function_args(BATCH_RUNNER, "main")
    assert rust["runner_init_args"] == _function_args(
        BATCH_RUNNER,
        "__init__",
        class_name="BatchRunner",
    )
    assert "fire.Fire(main)" in source
    assert "batch_{batch_num}.jsonl" in source
    assert "checkpoint.json" in source
    assert "statistics.json" in source
    for field in rust["result_fields"]:
        assert re.search(rf'["\']{re.escape(field)}["\']\s*:', source)


def test_mcp_boundary_matches_fastmcp_tool_surface() -> None:
    rust = _rust_snapshot()["mcp"]
    source = MCP_SERVE.read_text(encoding="utf-8")

    assert rust["server_name"] == "hermes"
    assert rust["tools"] == _mcp_tool_names()
    assert rust["queue_limit"] == _assigned_literal(MCP_SERVE, "QUEUE_LIMIT")
    assert rust["poll_interval_seconds"] == _assigned_literal(MCP_SERVE, "POLL_INTERVAL")
    for event_type in rust["event_types"]:
        assert event_type in source
    assert "run_stdio_async" in source


def test_rl_boundary_matches_cli_and_agent_invocation_contract() -> None:
    rust = _rust_snapshot()["rl"]
    source = RL_CLI.read_text(encoding="utf-8")

    assert rust["cli_args"] == _function_args(RL_CLI, "main")
    assert rust["toolsets"] == _assigned_literal(RL_CLI, "RL_TOOLSETS")
    assert rust["max_iterations"] == _assigned_literal(RL_CLI, "RL_MAX_ITERATIONS")
    for env in rust["required_env"]:
        assert env in source
    for env in rust["terminal_env"]:
        assert env in source
    for kwarg in rust["agent_kwargs"]:
        assert re.search(rf"\b{re.escape(kwarg)}\s*=", source)


def test_plugin_boundary_matches_registration_and_dashboard_helpers() -> None:
    rust = _rust_snapshot()["plugins"]
    source = PLUGINS.read_text(encoding="utf-8")

    assert rust["context_methods"] == _class_method_names(PLUGINS, "PluginContext")
    assert set(rust["valid_hooks"]) == _assigned_literal(PLUGINS, "VALID_HOOKS")
    assert rust["manifest_fields"] == _dataclass_fields(PLUGINS, "PluginManifest")
    for source_name in rust["discovery_sources"]:
        if source_name == "entry_points":
            assert "entry_points" in source
        else:
            assert f'source="{source_name}"' in source or f'source = "{source_name}"' in source
    assert set(rust["dashboard_helpers"]) <= _top_level_function_names(PLUGINS_CMD)


def test_boundary_document_covers_every_integration_surface() -> None:
    text = BOUNDARY_DOC.read_text(encoding="utf-8")

    for heading in ("Cron", "Batch Runner", "MCP", "RL", "Plugins"):
        assert f"## {heading}" in text
    for section in _rust_snapshot()["runtime_boundaries"]:
        assert section["surface"].split("_", 1)[0].lower() in text.lower()

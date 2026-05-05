"""Rust/Python parity for risk-ranked core tool handler slices."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_TOOLS_CRATE = REPO_ROOT / "crates" / "hermes-tools"
BOUNDARY_DOC = REPO_ROOT / "docs" / "rust-parity" / "tool-handler-boundaries.md"

pytestmark = pytest.mark.skipif(
    not RUST_TOOLS_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-tools handler snapshot not yet built; tracked by hermes-k77.4",
)


def _rust_snapshot(root: Path) -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-tools",
            "--bin",
            "hermes_tools_handlers_snapshot",
            str(root),
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust tools handler snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_snapshot(root: Path, monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    _prepare_fixture(root)

    import tools.file_tools as file_tools

    task_id = f"handler-parity-{os.getpid()}"
    monkeypatch.setenv("TERMINAL_CWD", str(root))
    file_tools.reset_file_dedup(task_id)
    with file_tools._read_tracker_lock:
        file_tools._read_tracker.pop(task_id, None)

    native = {}
    native["read_file_window"] = json.loads(
        file_tools.read_file_tool("sample.txt", offset=1, limit=2, task_id=task_id)
    )
    native["search_content"] = _normalize_search_result(
        file_tools.search_tool("alpha", path=".", target="content", limit=10, offset=0, task_id=task_id)
    )
    native["search_files"] = _normalize_search_result(
        file_tools.search_tool("*.txt", path=".", target="files", limit=10, offset=0, task_id=task_id)
    )
    native["write_file"] = json.loads(
        file_tools.write_file_tool("nested/new.txt", "hello\nworld\n", task_id=task_id)
    )
    native["protected_write"] = json.loads(
        file_tools.write_file_tool("/etc/hermes-agent-parity", "nope", task_id=task_id)
    )
    patch_result = json.loads(
        file_tools.patch_tool(
            path="sample.txt",
            old_string="beta alpha",
            new_string="BETA",
            replace_all=False,
            task_id=task_id,
        )
    )
    patch_result.pop("diff", None)
    patch_result.pop("_warning", None)
    native["patch_replace"] = patch_result
    native["patch_after_content"] = (root / "sample.txt").read_text(encoding="utf-8")

    return {
        "native_file_handlers": native,
        "python_boundaries": _documented_python_boundaries(),
    }


def _prepare_fixture(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "sample.txt").write_text("alpha\nbeta alpha\ngamma\n", encoding="utf-8")
    (root / "notes.md").write_text("# Notes\nalpha note\n", encoding="utf-8")
    (root / "src").mkdir(exist_ok=True)
    (root / "src" / "main.py").write_text("print('alpha')\n", encoding="utf-8")


def _normalize_search_result(result_text: str) -> dict[str, Any]:
    result = json.loads(result_text.split("\n\n[Hint:", 1)[0])
    if "matches" in result:
        result["matches"] = sorted(
            result["matches"],
            key=lambda item: (item["path"], item["line"], item["content"]),
        )
    if "files" in result:
        result["files"] = sorted(result["files"])
    return result


def _clean_root(name: str) -> Path:
    root = REPO_ROOT / "target" / name
    shutil.rmtree(root, ignore_errors=True)
    root.mkdir(parents=True, exist_ok=True)
    return root


def test_native_file_handler_snapshot_matches_python(monkeypatch: pytest.MonkeyPatch) -> None:
    rust_root = _clean_root(f"handler-parity-rust-{os.getpid()}")
    python_root = _clean_root(f"handler-parity-python-{os.getpid()}")

    assert _rust_snapshot(rust_root)["native_file_handlers"] == _python_snapshot(
        python_root,
        monkeypatch,
    )["native_file_handlers"]


def test_process_and_terminal_boundary_contracts() -> None:
    snapshot = _rust_snapshot(_clean_root(f"handler-boundary-rust-{os.getpid()}"))
    terminal = next(
        item for item in snapshot["python_boundaries"] if item["family"] == "terminal/process"
    )

    assert terminal["boundary"] == "python_runtime_boundary"
    assert set(terminal["tools"]) == {"terminal", "process"}
    assert "background process" in terminal["reason"]


def test_non_file_tool_families_have_documented_boundaries() -> None:
    snapshot = _rust_snapshot(_clean_root(f"handler-boundaries-rust-{os.getpid()}"))
    families = {item["family"]: item for item in snapshot["python_boundaries"]}

    assert {
        "terminal/process",
        "browser/web",
        "delegate/subagent",
        "mcp",
        "memory/todo",
        "media",
    } <= set(families)
    assert BOUNDARY_DOC.exists()
    doc = BOUNDARY_DOC.read_text(encoding="utf-8")
    for family in families:
        assert family in doc
    assert all(item["parity_gate"].endswith("test_handlers.py::test_non_file_tool_families_have_documented_boundaries")
               or item["family"] == "terminal/process"
               for item in snapshot["python_boundaries"])


def _documented_python_boundaries() -> list[dict[str, Any]]:
    return [
        {
            "family": "terminal/process",
            "boundary": "python_runtime_boundary",
            "tools": ["terminal", "process"],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_process_and_terminal_boundary_contracts",
            "reason": "Execution backends, PTY handling, background process reader threads, checkpoint recovery, and gateway watcher queues remain hosted by Python until the Rust daemon boundary owns process supervision.",
        },
        {
            "family": "browser/web",
            "boundary": "python_runtime_boundary",
            "tools": [
                "web_search",
                "web_extract",
                "browser_navigate",
                "browser_snapshot",
                "browser_click",
                "browser_type",
            ],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries",
            "reason": "Browser, search-provider, and extraction handlers depend on live Playwright/CDP sessions and external network/provider credentials; Rust currently preserves the Python boundary and validates schema/boundary coverage.",
        },
        {
            "family": "delegate/subagent",
            "boundary": "python_runtime_boundary",
            "tools": ["delegate_task"],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries",
            "reason": "Subagent execution inherits Python AIAgent lifecycle, approval callback propagation, and process-global toolset state; Rust parity is tracked at the agent loop and dispatch layers before this handler is cut over.",
        },
        {
            "family": "mcp",
            "boundary": "python_runtime_boundary",
            "tools": ["mcp:*"],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries",
            "reason": "MCP tools are dynamically discovered and refreshed at runtime from Python server adapters; Rust schema parity covers exposure while handler calls remain delegated to Python.",
        },
        {
            "family": "memory/todo",
            "boundary": "agent_loop_boundary",
            "tools": ["memory", "todo", "session_search"],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries",
            "reason": "These are intercepted by the agent loop and memory/session subsystems rather than registry-dispatched as ordinary tools; Rust dispatch parity explicitly preserves that boundary.",
        },
        {
            "family": "media",
            "boundary": "python_runtime_boundary",
            "tools": ["vision_analyze", "generate_image", "tts", "transcribe_audio"],
            "parity_gate": "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries",
            "reason": "Media handlers depend on optional provider SDKs, local binaries, and binary artifacts. They remain Python-hosted with schema/availability parity until provider-specific Rust clients are selected.",
        },
    ]

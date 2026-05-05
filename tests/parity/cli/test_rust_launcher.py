"""Rust launcher smoke tests for the full-parity runtime selector."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CLI_CRATE = REPO_ROOT / "crates" / "hermes-cli"

pytestmark = pytest.mark.skipif(
    not RUST_CLI_CRATE.exists() or shutil.which("cargo") is None,
    reason="Rust launcher requires cargo; tracked by hermes-fpr.2",
)


def _run_launcher(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    return subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-cli",
            "--bin",
            "hermes",
            "--",
            *args,
        ],
        cwd=REPO_ROOT,
        env=merged_env,
        capture_output=True,
        text=True,
        timeout=120,
    )


def _run_python_cli(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    return subprocess.run(
        [sys.executable, "-m", "hermes_cli.main", *args],
        cwd=REPO_ROOT,
        env=merged_env,
        capture_output=True,
        text=True,
        timeout=120,
    )


def _write_skill(profile_home: Path, name: str) -> None:
    skill_dir = profile_home / "skills" / "custom" / name
    skill_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text(f"---\nname: {name}\n---\n")


def test_runtime_info_reports_default_python_rollout_path() -> None:
    result = _run_launcher("--runtime-info")

    assert result.returncode == 0, result.stderr
    info = json.loads(result.stdout)
    assert info["selected_runtime"] == "python"
    assert info["default_runtime"] == "python"
    assert info["selector_env"] == "HERMES_RUNTIME"


def test_runtime_info_reports_explicit_rust_selection() -> None:
    result = _run_launcher("--runtime-info", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["selected_runtime"] == "rust"


def test_rust_runtime_has_native_smoke_command() -> None:
    result = _run_launcher("version", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 0, result.stderr
    assert result.stdout.startswith("hermes rust launcher ")


def test_rust_runtime_has_native_agent_runtime_smoke_command() -> None:
    result = _run_launcher("agent-runtime-smoke", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 0, result.stderr
    payload = json.loads(result.stdout)
    assert payload == {
        "ok": True,
        "final_message": "rust runtime smoke ok",
        "model_call_count": 2,
        "tool_call_count": 1,
        "message_count": 4,
    }


def test_rust_runtime_profile_status_matches_python_clean_profile(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    skill_dir = hermes_home / "skills" / "custom" / "alpha"
    ignored_dir = hermes_home / "skills" / ".hub" / "ignored"
    skill_dir.mkdir(parents=True)
    ignored_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text("---\nname: alpha\n---\n")
    (ignored_dir / "SKILL.md").write_text("---\nname: ignored\n---\n")
    (hermes_home / "config.yaml").write_text(
        "model:\n  default: gpt-test\n  provider: openai\n"
    )

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("profile", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("profile", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_profile_flag_matches_python_named_profile(tmp_path: Path) -> None:
    hermes_root = tmp_path / "hermes-root"
    profile_home = hermes_root / "profiles" / "coder"
    (profile_home / "skills" / "custom" / "coder").mkdir(parents=True)
    (profile_home / "skills" / "custom" / "coder" / "SKILL.md").write_text(
        "---\nname: coder\n---\n"
    )

    env = {"HERMES_HOME": str(hermes_root)}
    rust = _run_launcher("-p", "coder", "profile", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("-p", "coder", "profile", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_profile_list_matches_python(tmp_path: Path) -> None:
    hermes_root = tmp_path / "hermes-root"
    coder_home = hermes_root / "profiles" / "coder"
    _write_skill(hermes_root, "default-skill")
    _write_skill(coder_home, "coder-skill")
    (coder_home / "config.yaml").write_text("model: claude-test\n")

    env = {"HERMES_HOME": str(hermes_root)}
    rust = _run_launcher("profile", "list", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("profile", "list", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_profile_show_matches_python(tmp_path: Path) -> None:
    hermes_root = tmp_path / "hermes-root"
    coder_home = hermes_root / "profiles" / "coder"
    _write_skill(coder_home, "coder-skill")
    (coder_home / ".env").write_text("OPENAI_API_KEY=secret\n")
    (coder_home / "SOUL.md").write_text("profile soul\n")
    (coder_home / "config.yaml").write_text(
        "model:\n  default: gpt-show\n  provider: openai\n"
    )

    env = {"HERMES_HOME": str(hermes_root)}
    rust = _run_launcher("profile", "show", "coder", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("profile", "show", "coder", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_profile_use_matches_python(tmp_path: Path) -> None:
    rust_root = tmp_path / "rust-root"
    python_root = tmp_path / "python-root"
    (rust_root / "profiles" / "coder").mkdir(parents=True)
    (python_root / "profiles" / "coder").mkdir(parents=True)

    rust_env = {"HERMES_HOME": str(rust_root), "HERMES_RUNTIME": "rust"}
    python_env = {"HERMES_HOME": str(python_root)}
    rust = _run_launcher("profile", "use", "coder", env=rust_env)
    python = _run_python_cli("profile", "use", "coder", env=python_env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout
    assert (rust_root / "active_profile").read_text() == "coder\n"
    assert (python_root / "active_profile").read_text() == "coder\n"


def test_rust_runtime_gateway_status_matches_python_not_running(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("gateway", "status", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("gateway", "status", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_gateway_status_health_lines_match_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()
    (hermes_home / "gateway_state.json").write_text(
        json.dumps(
            {
                "gateway_state": "startup_failed",
                "exit_reason": "missing token",
                "active_agents": 0,
                "restart_requested": False,
                "platforms": {
                    "telegram": {
                        "state": "fatal",
                        "error_message": "bad credentials",
                    }
                },
            }
        )
    )

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("gateway", "status", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("gateway", "status", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_config_paths_match_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()

    env = {"HERMES_HOME": str(hermes_home)}
    for subcommand in ("path", "env-path"):
        rust = _run_launcher("config", subcommand, env={**env, "HERMES_RUNTIME": "rust"})
        python = _run_python_cli("config", subcommand, env=env)

        assert rust.returncode == 0, rust.stderr
        assert python.returncode == 0, python.stderr
        assert rust.stdout == python.stdout


def test_rust_runtime_config_paths_respect_profile_flag(tmp_path: Path) -> None:
    hermes_root = tmp_path / "hermes-root"
    profile_home = hermes_root / "profiles" / "coder"
    profile_home.mkdir(parents=True)

    env = {"HERMES_HOME": str(hermes_root)}
    rust = _run_launcher(
        "-p", "coder", "config", "path", env={**env, "HERMES_RUNTIME": "rust"}
    )
    python = _run_python_cli("-p", "coder", "config", "path", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_rejects_unported_commands_without_python_import() -> None:
    result = _run_launcher("gateway", "run", env={"HERMES_RUNTIME": "rust"})

    assert result.returncode == 78
    assert "not Rust-owned yet" in result.stderr
    assert "HERMES_RUNTIME=python" in result.stderr


def test_python_runtime_execs_explicit_fallback(tmp_path: Path) -> None:
    fake_python = tmp_path / "python"
    fake_python.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n")
    fake_python.chmod(fake_python.stat().st_mode | stat.S_IXUSR)

    result = _run_launcher(
        "version",
        env={"HERMES_RUNTIME": "python", "HERMES_PYTHON": str(fake_python)},
    )

    assert result.returncode == 0, result.stderr
    assert result.stdout.splitlines() == ["-m", "hermes_cli.main", "version"]


def test_install_script_builds_and_links_rust_launcher() -> None:
    script = (REPO_ROOT / "scripts" / "install.sh").read_text()

    assert "cargo build --release -p hermes-cli --bin hermes" in script
    assert 'RUST_HERMES_BIN="$INSTALL_DIR/target/release/hermes"' in script
    assert 'HERMES_BIN="$RUST_HERMES_BIN"' in script
    assert "HERMES_RUNTIME=python" in script


def test_update_flow_rebuilds_rust_launcher() -> None:
    main_py = (REPO_ROOT / "hermes_cli" / "main.py").read_text()

    assert "def _build_rust_hermes_launcher()" in main_py
    assert '"build", "--release", "-p", "hermes-cli", "--bin", "hermes"' in main_py
    assert "_build_rust_hermes_launcher()" in main_py
    assert "def _relink_rust_hermes_launcher" in main_py

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
import yaml

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


def _load_yaml(path: Path) -> dict:
    return yaml.safe_load(path.read_text()) or {}


def _write_skill_doc(home: Path, category: str, name: str, description: str) -> None:
    skill_dir = home / "skills" / category / name
    skill_dir.mkdir(parents=True, exist_ok=True)
    (skill_dir / "SKILL.md").write_text(
        f"---\nname: {name}\ndescription: {description}\n---\n"
    )


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


def test_rust_runtime_profile_delete_yes_matches_python(tmp_path: Path) -> None:
    rust_root = tmp_path / "rust-root"
    python_root = tmp_path / "python-root"
    for root in (rust_root, python_root):
        profile_home = root / "profiles" / "coder"
        _write_skill(profile_home, "coder-skill")
        (profile_home / "config.yaml").write_text("model:\n  default: gpt-delete\n")
        (root / "active_profile").write_text("coder\n")

    rust = _run_launcher(
        "profile",
        "delete",
        "coder",
        "-y",
        env={"HERMES_HOME": str(rust_root), "HERMES_RUNTIME": "rust"},
    )
    python = _run_python_cli(
        "profile", "delete", "coder", "-y", env={"HERMES_HOME": str(python_root)}
    )

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout.replace(str(rust_root), str(python_root)) == python.stdout
    assert not (rust_root / "profiles" / "coder").exists()
    assert not (python_root / "profiles" / "coder").exists()
    assert not (rust_root / "active_profile").exists()
    assert not (python_root / "active_profile").exists()


def test_rust_runtime_profile_rename_matches_python(tmp_path: Path) -> None:
    rust_root = tmp_path / "rust-root"
    python_root = tmp_path / "python-root"
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    for root in (rust_root, python_root):
        profile_home = root / "profiles" / "coderx"
        _write_skill(profile_home, "coder-skill")
        (profile_home / "config.yaml").write_text("model:\n  default: gpt-rename\n")
        (root / "active_profile").write_text("coderx\n")

    cargo_home = os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
    rustup_home = os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup"))
    rust_env = {
        "HERMES_HOME": str(rust_root),
        "HERMES_RUNTIME": "rust",
        "HOME": str(rust_home),
        "CARGO_HOME": cargo_home,
        "RUSTUP_HOME": rustup_home,
    }
    python_env = {"HERMES_HOME": str(python_root), "HOME": str(python_home)}
    rust = _run_launcher("profile", "rename", "coderx", "writerx", env=rust_env)
    python = _run_python_cli("profile", "rename", "coderx", "writerx", env=python_env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    normalized_rust_stdout = (
        rust.stdout.replace(str(rust_root), str(python_root))
        .replace(str(rust_home), str(python_home))
    )
    assert normalized_rust_stdout == python.stdout
    assert not (rust_root / "profiles" / "coderx").exists()
    assert not (python_root / "profiles" / "coderx").exists()
    assert (rust_root / "profiles" / "writerx").is_dir()
    assert (python_root / "profiles" / "writerx").is_dir()
    assert (rust_root / "active_profile").read_text() == "writerx\n"
    assert (python_root / "active_profile").read_text() == "writerx\n"
    assert (rust_home / ".local" / "bin" / "writerx").read_text() == (
        python_home / ".local" / "bin" / "writerx"
    ).read_text()


def test_rust_runtime_profile_alias_matches_python(tmp_path: Path) -> None:
    rust_root = tmp_path / "rust-root"
    python_root = tmp_path / "python-root"
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    for root in (rust_root, python_root):
        (root / "profiles" / "coderx").mkdir(parents=True)

    cargo_home = os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))
    rustup_home = os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup"))
    rust_wrapper_dir = rust_home / ".local" / "bin"
    python_wrapper_dir = python_home / ".local" / "bin"
    rust_env = {
        "HERMES_HOME": str(rust_root),
        "HERMES_RUNTIME": "rust",
        "HOME": str(rust_home),
        "PATH": f"{rust_wrapper_dir}{os.pathsep}{os.environ.get('PATH', '')}",
        "CARGO_HOME": cargo_home,
        "RUSTUP_HOME": rustup_home,
    }
    python_env = {
        "HERMES_HOME": str(python_root),
        "HOME": str(python_home),
        "PATH": f"{python_wrapper_dir}{os.pathsep}{os.environ.get('PATH', '')}",
    }

    rust = _run_launcher("profile", "alias", "coderx", "--name", "friendx", env=rust_env)
    python = _run_python_cli(
        "profile", "alias", "coderx", "--name", "friendx", env=python_env
    )

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout.replace(str(rust_home), str(python_home)) == python.stdout
    assert (rust_wrapper_dir / "friendx").read_text() == (
        python_wrapper_dir / "friendx"
    ).read_text()

    rust = _run_launcher(
        "profile", "alias", "coderx", "--name", "friendx", "--remove", env=rust_env
    )
    python = _run_python_cli(
        "profile", "alias", "coderx", "--name", "friendx", "--remove", env=python_env
    )

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout
    assert not (rust_wrapper_dir / "friendx").exists()
    assert not (python_wrapper_dir / "friendx").exists()


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


def test_rust_runtime_gateway_stop_matches_python_not_running(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("gateway", "stop", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("gateway", "stop", env=env)

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


def test_rust_runtime_config_set_matches_python(tmp_path: Path) -> None:
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    rust_home.mkdir()
    python_home.mkdir()

    rust_env = {"HERMES_HOME": str(rust_home), "HERMES_RUNTIME": "rust"}
    python_env = {"HERMES_HOME": str(python_home)}
    for args in (
        ("config", "set", "terminal.timeout", "123"),
        ("config", "set", "display.show_reasoning", "true"),
        ("config", "set", "model", "anthropic/claude-sonnet-4"),
        ("config", "set", "OPENAI_API_KEY", "sk-test"),
    ):
        rust = _run_launcher(*args, env=rust_env)
        python = _run_python_cli(*args, env=python_env)

        assert rust.returncode == 0, rust.stderr
        assert python.returncode == 0, python.stderr
        assert rust.stdout.replace(str(rust_home), str(python_home)) == python.stdout

    assert _load_yaml(rust_home / "config.yaml") == _load_yaml(python_home / "config.yaml")
    assert (rust_home / ".env").read_text() == (python_home / ".env").read_text()


def test_rust_runtime_config_set_list_index_matches_python(tmp_path: Path) -> None:
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    seed = "custom_providers:\n  - name: a\n    models:\n      - old\n"
    for home in (rust_home, python_home):
        home.mkdir()
        (home / "config.yaml").write_text(seed)

    rust = _run_launcher(
        "config",
        "set",
        "custom_providers.0.models.0",
        "new",
        env={"HERMES_HOME": str(rust_home), "HERMES_RUNTIME": "rust"},
    )
    python = _run_python_cli(
        "config",
        "set",
        "custom_providers.0.models.0",
        "new",
        env={"HERMES_HOME": str(python_home)},
    )

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout.replace(str(rust_home), str(python_home)) == python.stdout
    assert _load_yaml(rust_home / "config.yaml") == _load_yaml(python_home / "config.yaml")


def test_rust_runtime_config_set_respects_profile_flag(tmp_path: Path) -> None:
    rust_root = tmp_path / "rust-root"
    python_root = tmp_path / "python-root"
    (rust_root / "profiles" / "coder").mkdir(parents=True)
    (python_root / "profiles" / "coder").mkdir(parents=True)

    rust = _run_launcher(
        "-p",
        "coder",
        "config",
        "set",
        "terminal.timeout",
        "45",
        env={"HERMES_HOME": str(rust_root), "HERMES_RUNTIME": "rust"},
    )
    python = _run_python_cli(
        "-p",
        "coder",
        "config",
        "set",
        "terminal.timeout",
        "45",
        env={"HERMES_HOME": str(python_root)},
    )

    rust_profile = rust_root / "profiles" / "coder"
    python_profile = python_root / "profiles" / "coder"
    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout.replace(str(rust_profile), str(python_profile)) == python.stdout
    assert _load_yaml(rust_profile / "config.yaml") == _load_yaml(
        python_profile / "config.yaml"
    )
    assert (rust_profile / ".env").read_text() == (python_profile / ".env").read_text()


def test_rust_runtime_logs_list_matches_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    logs_dir = hermes_home / "logs"
    logs_dir.mkdir(parents=True)
    (logs_dir / "agent.log").write_text("agent line\n")
    (logs_dir / "errors.log").write_text("")
    (logs_dir / "gateway.log").write_text("gateway line\n")

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("logs", "list", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("logs", "list", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_logs_tail_filters_match_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    logs_dir = hermes_home / "logs"
    logs_dir.mkdir(parents=True)
    (logs_dir / "agent.log").write_text(
        "\n".join(
            [
                "2026-05-05 10:00:00 INFO run_agent: hello abc",
                "2026-05-05 10:00:01 WARNING tools.file: warn abc",
                "2026-05-05 10:00:02 ERROR gateway.run: wrong component abc",
                "2026-05-05 10:00:03 ERROR tools.memory: boom xyz",
            ]
        )
        + "\n"
    )

    env = {"HERMES_HOME": str(hermes_home)}
    args = ("logs", "--level", "WARNING", "--session", "abc", "--component", "tools", "-n", "5")
    rust = _run_launcher(*args, env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli(*args, env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_cron_status_matches_python_no_jobs(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("cron", "status", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("cron", "status", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_cron_status_matches_python_active_jobs(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    cron_dir = hermes_home / "cron"
    cron_dir.mkdir(parents=True)
    (cron_dir / "jobs.json").write_text(
        json.dumps(
            {
                "jobs": [
                    {
                        "id": "job-a",
                        "enabled": True,
                        "next_run_at": "2026-05-06T09:00:00+00:00",
                    },
                    {
                        "id": "job-b",
                        "enabled": True,
                        "next_run_at": "2026-05-05T09:00:00+00:00",
                    },
                    {
                        "id": "job-c",
                        "enabled": False,
                        "next_run_at": "2026-05-04T09:00:00+00:00",
                    },
                ]
            }
        )
    )

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("cron", "status", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("cron", "status", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_cron_list_matches_python_no_jobs(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    hermes_home.mkdir()

    env = {"HERMES_HOME": str(hermes_home)}
    rust = _run_launcher("cron", "list", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("cron", "list", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_cron_list_matches_python_jobs_and_all_flag(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    cron_dir = hermes_home / "cron"
    cron_dir.mkdir(parents=True)
    (cron_dir / "jobs.json").write_text(
        json.dumps(
            {
                "jobs": [
                    {
                        "id": "job-a",
                        "name": "Alpha",
                        "schedule_display": "every 5 minutes",
                        "schedule": {"value": "5m"},
                        "state": "scheduled",
                        "enabled": True,
                        "next_run_at": "2026-05-06T09:00:00+00:00",
                        "repeat": {"times": 3, "completed": 1},
                        "deliver": ["local", "telegram"],
                        "skills": ["foo", "bar"],
                        "script": "/tmp/job.py",
                        "workdir": "/tmp",
                        "last_status": "ok",
                        "last_run_at": "2026-05-05T09:00:00+00:00",
                    },
                    {
                        "id": "job-b",
                        "name": "Beta",
                        "schedule": {"value": "1h"},
                        "enabled": False,
                        "next_run_at": "2026-05-06T10:00:00+00:00",
                        "last_status": "error",
                        "last_error": "bad",
                        "last_run_at": "2026-05-05T10:00:00+00:00",
                        "last_delivery_error": "send failed",
                    },
                    {
                        "id": "job-c",
                        "skill": "legacy",
                        "enabled": True,
                        "schedule": {"value": "?"},
                        "repeat": {},
                        "deliver": "slack",
                    },
                ]
            }
        )
    )

    env = {"HERMES_HOME": str(hermes_home)}
    for args in [("cron", "list"), ("cron", "list", "--all")]:
        rust = _run_launcher(*args, env={**env, "HERMES_RUNTIME": "rust"})
        python = _run_python_cli(*args, env=env)

        assert rust.returncode == 0, rust.stderr
        assert python.returncode == 0, python.stderr
        assert rust.stdout == python.stdout


def test_rust_runtime_plugins_list_matches_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    bundled = tmp_path / "bundled-plugins"
    user_plugins = hermes_home / "plugins" / "user-one"
    bundled_one = bundled / "bundled-one"
    user_plugins.mkdir(parents=True)
    bundled_one.mkdir(parents=True)
    (bundled_one / "plugin.yaml").write_text(
        "name: bundled-one\nversion: 0.1\ndescription: Bundled one\n"
    )
    (user_plugins / "plugin.yaml").write_text(
        "name: user-one\nversion: 1.2\ndescription: User one\n"
    )
    (hermes_home / "config.yaml").write_text(
        "plugins:\n  enabled:\n    - user-one\n  disabled:\n    - bundled-one\n"
    )

    env = {
        "HERMES_HOME": str(hermes_home),
        "HERMES_BUNDLED_PLUGINS": str(bundled),
    }
    rust = _run_launcher("plugins", "list", env={**env, "HERMES_RUNTIME": "rust"})
    python = _run_python_cli("plugins", "list", env=env)

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout


def test_rust_runtime_plugins_enable_disable_match_python(tmp_path: Path) -> None:
    bundled = tmp_path / "bundled-plugins"
    (bundled / "bundled-one").mkdir(parents=True)
    (bundled / "bundled-one" / "plugin.yaml").write_text(
        "name: bundled-one\nversion: 0.1\ndescription: Bundled one\n"
    )
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    rust_home.mkdir()
    python_home.mkdir()

    rust_env = {
        "HERMES_HOME": str(rust_home),
        "HERMES_BUNDLED_PLUGINS": str(bundled),
        "HERMES_RUNTIME": "rust",
    }
    python_env = {
        "HERMES_HOME": str(python_home),
        "HERMES_BUNDLED_PLUGINS": str(bundled),
    }

    rust_enable = _run_launcher("plugins", "enable", "bundled-one", env=rust_env)
    python_enable = _run_python_cli("plugins", "enable", "bundled-one", env=python_env)
    assert rust_enable.returncode == 0, rust_enable.stderr
    assert python_enable.returncode == 0, python_enable.stderr
    assert rust_enable.stdout == python_enable.stdout

    rust_disable = _run_launcher("plugins", "disable", "bundled-one", env=rust_env)
    python_disable = _run_python_cli("plugins", "disable", "bundled-one", env=python_env)
    assert rust_disable.returncode == 0, rust_disable.stderr
    assert python_disable.returncode == 0, python_disable.stderr
    assert rust_disable.stdout == python_disable.stdout

    rust_cfg = _load_yaml(rust_home / "config.yaml")
    python_cfg = _load_yaml(python_home / "config.yaml")
    assert rust_cfg["plugins"] == python_cfg["plugins"]


def test_rust_runtime_plugins_missing_matches_python(tmp_path: Path) -> None:
    hermes_home = tmp_path / "hermes-home"
    bundled = tmp_path / "bundled-plugins"
    hermes_home.mkdir()

    env = {
        "HERMES_HOME": str(hermes_home),
        "HERMES_BUNDLED_PLUGINS": str(bundled),
    }
    rust = _run_launcher(
        "plugins", "enable", "missing-plugin", env={**env, "HERMES_RUNTIME": "rust"}
    )
    python = _run_python_cli("plugins", "enable", "missing-plugin", env=env)

    assert rust.returncode == python.returncode == 1
    assert rust.stdout == python.stdout
    assert rust.stderr == python.stderr == ""


def test_rust_runtime_skills_list_matches_python_empty_home(tmp_path: Path) -> None:
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    rust_home.mkdir()
    python_home.mkdir()

    rust = _run_launcher(
        "skills", "list", env={"HERMES_HOME": str(rust_home), "HERMES_RUNTIME": "rust"}
    )
    python = _run_python_cli("skills", "list", env={"HERMES_HOME": str(python_home)})

    assert rust.returncode == 0, rust.stderr
    assert python.returncode == 0, python.stderr
    assert rust.stdout == python.stdout
    for home in (rust_home, python_home):
        assert (home / "skills" / ".hub" / "lock.json").exists()
        assert (home / "skills" / ".hub" / "quarantine").is_dir()
        assert (home / "skills" / ".hub" / "index-cache").is_dir()


def test_rust_runtime_skills_list_sources_match_python(tmp_path: Path) -> None:
    rust_home = tmp_path / "rust-home"
    python_home = tmp_path / "python-home"
    for home in (rust_home, python_home):
        _write_skill_doc(home, "x", "hub-skill", "hub")
        _write_skill_doc(home, "x", "builtin-skill", "builtin")
        _write_skill_doc(home, "x", "local-skill", "local")
        (home / "skills" / ".hub").mkdir(parents=True, exist_ok=True)
        (home / "skills" / ".bundled_manifest").write_text("builtin-skill:abc123\n")
        (home / "skills" / ".hub" / "lock.json").write_text(
            json.dumps(
                {
                    "version": 1,
                    "installed": {
                        "hub-skill": {
                            "source": "github",
                            "identifier": "id",
                            "trust_level": "community",
                            "scan_verdict": "pass",
                            "content_hash": "hash",
                            "install_path": "x/hub-skill",
                            "files": [],
                        }
                    },
                }
            )
        )
        (home / "config.yaml").write_text("skills:\n  disabled:\n    - hub-skill\n")

    rust_env = {"HERMES_HOME": str(rust_home), "HERMES_RUNTIME": "rust"}
    python_env = {"HERMES_HOME": str(python_home)}
    for args in (
        ("skills", "list"),
        ("skills", "list", "--enabled-only"),
        ("skills", "list", "--source", "local"),
        ("skills", "list", "--source=hub"),
    ):
        rust = _run_launcher(*args, env=rust_env)
        python = _run_python_cli(*args, env=python_env)

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

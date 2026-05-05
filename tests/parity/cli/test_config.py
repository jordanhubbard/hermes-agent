"""Rust/Python parity for config merge and profile path semantics."""

from __future__ import annotations

import copy
import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
RUST_CONFIG_CRATE = REPO_ROOT / "crates" / "hermes-config"


pytestmark = pytest.mark.skipif(
    not RUST_CONFIG_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-config not yet built; tracked by hermes-3n2.2",
)


def _rust_probe(case: dict) -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-config",
            "--bin",
            "hermes_config_probe",
        ],
        cwd=REPO_ROOT,
        input=json.dumps(case),
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust config probe failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_probe(case: dict, monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> dict:
    home = Path(case["home"])
    hermes_home = case.get("hermes_home")
    if hermes_home:
        monkeypatch.setenv("HERMES_HOME", hermes_home)
    else:
        monkeypatch.delenv("HERMES_HOME", raising=False)
    monkeypatch.setattr(Path, "home", lambda: home)
    for key, value in case.get("env", {}).items():
        monkeypatch.setenv(key, value)

    from hermes_constants import (
        display_hermes_home,
        get_default_hermes_root,
        get_hermes_home,
    )
    from hermes_cli.config import (
        _deep_merge,
        _expand_env_vars,
        _normalize_max_turns_config,
        _normalize_root_model_keys,
    )

    default_config = copy.deepcopy(case["default_config"])
    user_config = copy.deepcopy(case["user_config"])
    if "max_turns" in user_config:
        agent_user_config = dict(user_config.get("agent") or {})
        if agent_user_config.get("max_turns") is None:
            agent_user_config["max_turns"] = user_config["max_turns"]
        user_config["agent"] = agent_user_config
        user_config.pop("max_turns", None)
    loaded = _deep_merge(default_config, user_config)
    loaded = _normalize_root_model_keys(_normalize_max_turns_config(loaded))
    loaded = _expand_env_vars(loaded)

    default_root = get_default_hermes_root()
    paths = {
        "hermes_home": str(get_hermes_home()),
        "default_hermes_root": str(default_root),
        "profiles_root": str(default_root / "profiles"),
        "active_profile_path": str(default_root / "active_profile"),
        "display_hermes_home": display_hermes_home(),
    }
    return {
        "paths": paths,
        "loaded_config": loaded,
        "cli_env_bridge": _expected_cli_bridge(loaded, case["current_dir"]),
        "gateway_env_bridge": _expected_gateway_bridge(case["user_config"], case.get("env", {})),
    }


def _json_value_to_env(value) -> str | None:
    if value is None:
        return None
    if isinstance(value, bool):
        return str(value)
    if isinstance(value, (int, float, str)):
        return str(value)
    return json.dumps(value)


def _expand_value(value, env: dict):
    if isinstance(value, str):
        for key, replacement in env.items():
            value = value.replace("${" + key + "}", replacement)
        return value
    if isinstance(value, list):
        return [_expand_value(item, env) for item in value]
    if isinstance(value, dict):
        return {key: _expand_value(item, env) for key, item in value.items()}
    return value


def _expected_cli_bridge(config: dict, current_dir: str) -> dict:
    terminal = config.get("terminal") or {}
    bridge = {}
    backend = str(terminal.get("env_type") or terminal.get("backend") or "local")
    bridge["TERMINAL_ENV"] = backend
    if backend == "local":
        bridge["TERMINAL_CWD"] = current_dir
    else:
        cwd = terminal.get("cwd")
        if cwd not in (None, ".", "auto", "cwd"):
            bridge["TERMINAL_CWD"] = str(cwd)
    for key, env_name in {
        "timeout": "TERMINAL_TIMEOUT",
        "docker_forward_env": "TERMINAL_DOCKER_FORWARD_ENV",
        "container_cpu": "TERMINAL_CONTAINER_CPU",
        "container_persistent": "TERMINAL_CONTAINER_PERSISTENT",
    }.items():
        if key in terminal:
            rendered = _json_value_to_env(terminal[key])
            if rendered is not None:
                bridge[env_name] = rendered
    return dict(sorted(bridge.items()))


def _expected_gateway_bridge(raw_config: dict, env: dict) -> dict:
    expanded = _expand_value(raw_config, env)
    bridge = {}
    for key, value in expanded.items():
        if isinstance(value, (str, int, float, bool)) and key not in env:
            bridge[key] = _json_value_to_env(value)
    terminal = expanded.get("terminal") or {}
    for key, env_name in {
        "backend": "TERMINAL_ENV",
        "cwd": "TERMINAL_CWD",
        "timeout": "TERMINAL_TIMEOUT",
        "docker_forward_env": "TERMINAL_DOCKER_FORWARD_ENV",
        "container_cpu": "TERMINAL_CONTAINER_CPU",
        "container_persistent": "TERMINAL_CONTAINER_PERSISTENT",
    }.items():
        if key not in terminal:
            continue
        value = terminal[key]
        if key == "cwd":
            if str(value) in (".", "auto", "cwd"):
                continue
            value = str(value).replace("~/", env.get("HOME", "~") + "/", 1)
        rendered = _json_value_to_env(value)
        if rendered is not None:
            bridge[env_name] = rendered
    agent = expanded.get("agent") or {}
    for key, env_name in {
        "max_turns": "HERMES_MAX_ITERATIONS",
        "gateway_timeout": "HERMES_AGENT_TIMEOUT",
        "gateway_timeout_warning": "HERMES_AGENT_TIMEOUT_WARNING",
        "gateway_notify_interval": "HERMES_AGENT_NOTIFY_INTERVAL",
        "restart_drain_timeout": "HERMES_RESTART_DRAIN_TIMEOUT",
        "gateway_auto_continue_freshness": "HERMES_AUTO_CONTINUE_FRESHNESS",
    }.items():
        if key in agent:
            rendered = _json_value_to_env(agent[key])
            if rendered is not None:
                bridge[env_name] = rendered
    return dict(sorted(bridge.items()))


@pytest.mark.parametrize(
    "case_name, hermes_home",
    [
        ("default", None),
        ("native_profile", "{home}/.hermes/profiles/coder"),
        ("custom_root", "{home}/opt/data"),
        ("custom_profile", "{home}/opt/data/profiles/coder"),
    ],
)
def test_rust_config_profile_paths_and_merge_match_python(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path, case_name: str, hermes_home: str | None
) -> None:
    home = tmp_path / "home"
    home.mkdir()
    resolved_hermes_home = hermes_home.format(home=home) if hermes_home else None
    if resolved_hermes_home:
        Path(resolved_hermes_home).mkdir(parents=True)
    case = {
        "home": str(home),
        "hermes_home": resolved_hermes_home,
        "current_dir": str(tmp_path / "work"),
        "env": {
            "BASE_URL": "https://api.example.test",
            "TOKEN": "secret-token",
            "HOME": str(home),
        },
        "default_config": {
            "model": "",
            "providers": {},
            "toolsets": ["hermes-cli"],
            "agent": {
                "max_turns": 90,
                "gateway_timeout": 1800,
                "gateway_timeout_warning": 900,
                "gateway_notify_interval": 180,
                "restart_drain_timeout": 180,
                "gateway_auto_continue_freshness": 3600,
            },
            "terminal": {
                "backend": "local",
                "cwd": ".",
                "timeout": 180,
                "docker_forward_env": [],
                "container_cpu": 1,
                "container_persistent": True,
            },
        },
        "user_config": {
            "max_turns": 12,
            "provider": "openai",
            "base_url": "${BASE_URL}",
            "plain": "${TOKEN}",
            "agent": {
                "gateway_timeout": 77,
                "gateway_notify_interval": 9,
            },
            "terminal": {
                "backend": "docker",
                "cwd": "~/workspace",
                "timeout": 30,
                "docker_forward_env": ["PATH", "HOME"],
                "container_cpu": 2,
            },
        },
    }

    assert _rust_probe(case) == _python_probe(case, monkeypatch, tmp_path), case_name

"""Rust/Python/React parity for the dashboard backend API contract."""

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
RUST_DASHBOARD_CRATE = REPO_ROOT / "crates" / "hermes-dashboard"
WEB_SERVER = REPO_ROOT / "hermes_cli" / "web_server.py"
WEB_SRC = REPO_ROOT / "web" / "src"
API_TS = WEB_SRC / "lib" / "api.ts"
CHAT_PAGE = WEB_SRC / "pages" / "ChatPage.tsx"
CHAT_SIDEBAR = WEB_SRC / "components" / "ChatSidebar.tsx"
GATEWAY_CLIENT = WEB_SRC / "lib" / "gatewayClient.ts"

pytestmark = pytest.mark.skipif(
    not RUST_DASHBOARD_CRATE.exists() or shutil.which("cargo") is None,
    reason="crates/hermes-dashboard not yet built; tracked by hermes-dwg.3",
)


def _rust_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "hermes-dashboard",
            "--bin",
            "hermes_dashboard_snapshot",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, (
        f"Rust dashboard snapshot failed: stdout={result.stdout!r} "
        f"stderr={result.stderr!r}"
    )
    return json.loads(result.stdout)


def _python_routes() -> set[tuple[str, str]]:
    source = WEB_SERVER.read_text(encoding="utf-8")
    pattern = re.compile(
        r"@app\.(get|post|put|delete|patch|websocket)\(\s*['\"]([^'\"]+)['\"]"
    )
    return {(method.upper(), path) for method, path in pattern.findall(source)}


def _python_public_api_paths() -> set[str]:
    source = WEB_SERVER.read_text(encoding="utf-8")
    match = re.search(
        r"_PUBLIC_API_PATHS:\s*frozenset\s*=\s*frozenset\(\s*(\{.*?\})\s*\)",
        source,
        re.S,
    )
    assert match, "_PUBLIC_API_PATHS not found in hermes_cli/web_server.py"
    return set(ast.literal_eval(match.group(1)))


def _shape(path: str) -> str:
    return re.sub(r"\{[^}]+\}", "{}", path.split("?", 1)[0])


def _api_client_paths() -> set[str]:
    source = API_TS.read_text(encoding="utf-8")
    paths: set[str] = set()
    for quote, raw in re.findall(r"fetchJSON(?:<[^>]+>)?\(\s*([`'\"])(.*?)(?<!\\)\1", source, re.S):
        del quote
        if not raw.startswith("/"):
            continue
        path = re.sub(r"\$\{[^}]+\}", "{}", raw)
        paths.add(_shape(path))
    return paths


def test_route_table_matches_fastapi_decorators() -> None:
    rust = _rust_snapshot()
    rust_routes = {(route["method"], route["path"]) for route in rust["routes"]}
    rust_ws = {("WEBSOCKET", route["path"]) for route in rust["websockets"]}

    assert rust_routes | rust_ws == _python_routes()


def test_auth_public_paths_match_middleware_contract() -> None:
    rust = _rust_snapshot()
    public_paths = set(rust["middleware"]["public_api_paths"])

    assert public_paths == _python_public_api_paths()
    assert rust["middleware"]["session_header"] == "X-Hermes-Session-Token"
    assert rust["middleware"]["api_plugin_prefix_exempt"] is True
    assert rust["middleware"]["host_header_guard"] is True
    assert rust["middleware"]["localhost_cors_only"] is True

    for route in rust["routes"]:
        path = route["path"]
        if path in public_paths:
            assert route["auth"] == "public"
        elif path.startswith("/api/"):
            assert route["auth"] == "session_token"


def test_react_api_client_paths_are_covered_by_dashboard_routes() -> None:
    rust = _rust_snapshot()
    route_shapes = {_shape(route["path"]) for route in rust["routes"]}
    client_paths = _api_client_paths()

    assert client_paths <= route_shapes
    assert "/api/cron/jobs/{}/pause" in client_paths
    assert "/api/dashboard/agent-plugins/{}/enable" in client_paths
    assert "/api/providers/oauth/{}/poll/{}" in client_paths


def test_embedded_chat_keeps_xterm_pty_as_primary_chat_surface() -> None:
    rust_chat = _rust_snapshot()["embedded_chat"]
    web_server = WEB_SERVER.read_text(encoding="utf-8")
    chat_page = CHAT_PAGE.read_text(encoding="utf-8")
    chat_sidebar = CHAT_SIDEBAR.read_text(encoding="utf-8")
    gateway_client = GATEWAY_CLIENT.read_text(encoding="utf-8")

    assert rust_chat["pty_websocket"] == "/api/pty"
    assert rust_chat["terminal_library"] == "@xterm/xterm"
    assert rust_chat["resume_env"] == "HERMES_TUI_RESUME"
    assert rust_chat["sidecar_env"] == "HERMES_TUI_SIDECAR_URL"

    assert "_make_tui_argv(PROJECT_ROOT / \"ui-tui\", tui_dev=False)" in web_server
    assert "PtyBridge.spawn" in web_server
    assert "_RESIZE_RE" in web_server
    assert "HERMES_TUI_RESUME" in web_server
    assert "HERMES_TUI_SIDECAR_URL" in web_server

    assert "@xterm/xterm" in chat_page
    assert "new Terminal(" in chat_page
    assert "/api/pty" in chat_page
    assert "term.onData" in chat_page
    assert "term.onResize" in chat_page
    assert "prompt.submit" not in chat_page
    assert "executeSlash" not in chat_page

    assert "/api/events" in chat_sidebar
    assert "/api/ws" in gateway_client
    assert "prompt.submit" in gateway_client


def test_websocket_contract_covers_close_codes_and_channels() -> None:
    rust_ws = {ws["path"]: ws for ws in _rust_snapshot()["websockets"]}

    assert set(rust_ws) == {"/api/pty", "/api/ws", "/api/pub", "/api/events"}
    assert rust_ws["/api/pty"]["channel_required"] is False
    assert rust_ws["/api/pub"]["channel_required"] is True
    assert rust_ws["/api/events"]["channel_required"] is True

    for ws in rust_ws.values():
        assert ws["auth"] == "query_token_and_loopback"
        assert ws["enabled_flag"] == "_DASHBOARD_EMBEDDED_CHAT_ENABLED"
        assert 4401 in ws["close_codes"]
        assert 4403 in ws["close_codes"]

    assert 4400 in rust_ws["/api/pub"]["close_codes"]
    assert 4400 in rust_ws["/api/events"]["close_codes"]
    assert 1011 in rust_ws["/api/pty"]["close_codes"]

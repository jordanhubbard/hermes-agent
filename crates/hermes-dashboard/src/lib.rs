use std::collections::BTreeSet;

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DashboardRoute {
    pub method: String,
    pub path: String,
    pub group: String,
    pub auth: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DashboardWebSocket {
    pub path: String,
    pub purpose: String,
    pub auth: String,
    pub enabled_flag: String,
    pub close_codes: Vec<u16>,
    pub channel_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DashboardMiddleware {
    pub session_header: String,
    pub auth_scope: String,
    pub public_api_paths: Vec<String>,
    pub api_plugin_prefix_exempt: bool,
    pub host_header_guard: bool,
    pub localhost_cors_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EmbeddedChatContract {
    pub primary_surface: String,
    pub frontend_file: String,
    pub terminal_library: String,
    pub pty_websocket: String,
    pub pty_bridge: String,
    pub tui_argv_resolver: String,
    pub resume_env: String,
    pub sidecar_env: String,
    pub sidecar_publish_ws: String,
    pub sidecar_events_ws: String,
    pub metadata_ws: String,
    pub resize_escape_prefix: String,
    pub prohibited_primary_chat_paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuntimeBoundary {
    pub surface: String,
    pub boundary: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DashboardSnapshot {
    pub protocol: String,
    pub routes: Vec<DashboardRoute>,
    pub websockets: Vec<DashboardWebSocket>,
    pub middleware: DashboardMiddleware,
    pub embedded_chat: EmbeddedChatContract,
    pub runtime_boundaries: Vec<RuntimeBoundary>,
}

pub fn dashboard_snapshot() -> DashboardSnapshot {
    DashboardSnapshot {
        protocol: "hermes-dashboard.fastapi.v1".to_string(),
        routes: http_routes(),
        websockets: websocket_routes(),
        middleware: DashboardMiddleware {
            session_header: "X-Hermes-Session-Token".to_string(),
            auth_scope: "all /api/ routes except public list and /api/plugins/*".to_string(),
            public_api_paths: strings(PUBLIC_API_PATHS),
            api_plugin_prefix_exempt: true,
            host_header_guard: true,
            localhost_cors_only: true,
        },
        embedded_chat: EmbeddedChatContract {
            primary_surface: "embedded hermes --tui rendered through xterm.js over a PTY WebSocket".to_string(),
            frontend_file: "web/src/pages/ChatPage.tsx".to_string(),
            terminal_library: "@xterm/xterm".to_string(),
            pty_websocket: "/api/pty".to_string(),
            pty_bridge: "hermes_cli.pty_bridge.PtyBridge.spawn".to_string(),
            tui_argv_resolver: "hermes_cli.main._make_tui_argv(PROJECT_ROOT / \"ui-tui\", tui_dev=False)".to_string(),
            resume_env: "HERMES_TUI_RESUME".to_string(),
            sidecar_env: "HERMES_TUI_SIDECAR_URL".to_string(),
            sidecar_publish_ws: "/api/pub".to_string(),
            sidecar_events_ws: "/api/events".to_string(),
            metadata_ws: "/api/ws".to_string(),
            resize_escape_prefix: "\u{1b}[RESIZE:".to_string(),
            prohibited_primary_chat_paths: strings(&[
                "React transcript renderer",
                "React composer",
                "prompt.submit as the primary message transport",
            ]),
        },
        runtime_boundaries: vec![
            RuntimeBoundary {
                surface: "Dashboard REST API".to_string(),
                boundary: "rust_contract_python_runtime".to_string(),
                reason: "Rust owns the route/auth/client-use contract while existing FastAPI handlers remain the runtime for config, env, sessions, models, cron, profiles, plugins, analytics, and action subprocesses.".to_string(),
            },
            RuntimeBoundary {
                surface: "Dashboard chat tab".to_string(),
                boundary: "pty_embedded_tui".to_string(),
                reason: "The production dashboard must continue to spawn the real Hermes TUI through a PTY and render ANSI with xterm.js. React sidebars may consume metadata/events, but they do not replace the transcript or composer.".to_string(),
            },
            RuntimeBoundary {
                surface: "Dashboard plugin APIs".to_string(),
                boundary: "dynamic_fastapi_router".to_string(),
                reason: "Plugin routers are mounted dynamically under /api/plugins/<name>/ and are explicitly outside the static built-in route table; plugin-specific contracts stay with each plugin.".to_string(),
            },
        ],
    }
}

pub fn route_shapes(routes: &[DashboardRoute]) -> BTreeSet<String> {
    routes.iter().map(|r| route_shape(&r.path)).collect()
}

pub fn route_shape(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            out.push_str("{}");
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn is_public_api_path(path: &str) -> bool {
    PUBLIC_API_PATHS.contains(&path)
}

fn http_routes() -> Vec<DashboardRoute> {
    vec![
        route("GET", "/api/status", "status"),
        route("POST", "/api/gateway/restart", "actions"),
        route("POST", "/api/hermes/update", "actions"),
        route("GET", "/api/actions/{name}/status", "actions"),
        route("GET", "/api/sessions", "sessions"),
        route("GET", "/api/sessions/search", "sessions"),
        route("GET", "/api/config", "config"),
        route("GET", "/api/config/defaults", "config"),
        route("GET", "/api/config/schema", "config"),
        route("GET", "/api/model/info", "models"),
        route("GET", "/api/model/options", "models"),
        route("GET", "/api/model/auxiliary", "models"),
        route("POST", "/api/model/set", "models"),
        route("PUT", "/api/config", "config"),
        route("GET", "/api/env", "env"),
        route("PUT", "/api/env", "env"),
        route("DELETE", "/api/env", "env"),
        route("POST", "/api/env/reveal", "env"),
        route("GET", "/api/providers/oauth", "oauth"),
        route("DELETE", "/api/providers/oauth/{provider_id}", "oauth"),
        route("POST", "/api/providers/oauth/{provider_id}/start", "oauth"),
        route("POST", "/api/providers/oauth/{provider_id}/submit", "oauth"),
        route(
            "GET",
            "/api/providers/oauth/{provider_id}/poll/{session_id}",
            "oauth",
        ),
        route(
            "DELETE",
            "/api/providers/oauth/sessions/{session_id}",
            "oauth",
        ),
        route("GET", "/api/sessions/{session_id}", "sessions"),
        route("GET", "/api/sessions/{session_id}/messages", "sessions"),
        route("DELETE", "/api/sessions/{session_id}", "sessions"),
        route("GET", "/api/logs", "logs"),
        route("GET", "/api/cron/jobs", "cron"),
        route("GET", "/api/cron/jobs/{job_id}", "cron"),
        route("POST", "/api/cron/jobs", "cron"),
        route("PUT", "/api/cron/jobs/{job_id}", "cron"),
        route("POST", "/api/cron/jobs/{job_id}/pause", "cron"),
        route("POST", "/api/cron/jobs/{job_id}/resume", "cron"),
        route("POST", "/api/cron/jobs/{job_id}/trigger", "cron"),
        route("DELETE", "/api/cron/jobs/{job_id}", "cron"),
        route("GET", "/api/profiles", "profiles"),
        route("POST", "/api/profiles", "profiles"),
        route("GET", "/api/profiles/{name}/setup-command", "profiles"),
        route("POST", "/api/profiles/{name}/open-terminal", "profiles"),
        route("PATCH", "/api/profiles/{name}", "profiles"),
        route("DELETE", "/api/profiles/{name}", "profiles"),
        route("GET", "/api/profiles/{name}/soul", "profiles"),
        route("PUT", "/api/profiles/{name}/soul", "profiles"),
        route("GET", "/api/skills", "skills"),
        route("PUT", "/api/skills/toggle", "skills"),
        route("GET", "/api/tools/toolsets", "tools"),
        route("GET", "/api/config/raw", "config"),
        route("PUT", "/api/config/raw", "config"),
        route("GET", "/api/analytics/usage", "analytics"),
        route("GET", "/api/analytics/models", "analytics"),
        route("GET", "/api/dashboard/themes", "dashboard"),
        route("PUT", "/api/dashboard/theme", "dashboard"),
        route("GET", "/api/dashboard/plugins", "dashboard"),
        route("GET", "/api/dashboard/plugins/rescan", "dashboard"),
        route("GET", "/api/dashboard/plugins/hub", "dashboard"),
        route("POST", "/api/dashboard/agent-plugins/install", "dashboard"),
        route(
            "POST",
            "/api/dashboard/agent-plugins/{name}/enable",
            "dashboard",
        ),
        route(
            "POST",
            "/api/dashboard/agent-plugins/{name}/disable",
            "dashboard",
        ),
        route(
            "POST",
            "/api/dashboard/agent-plugins/{name}/update",
            "dashboard",
        ),
        route("DELETE", "/api/dashboard/agent-plugins/{name}", "dashboard"),
        route("PUT", "/api/dashboard/plugin-providers", "dashboard"),
        route(
            "POST",
            "/api/dashboard/plugins/{name}/visibility",
            "dashboard",
        ),
        route(
            "GET",
            "/dashboard-plugins/{plugin_name}/{file_path:path}",
            "dashboard_plugins",
        ),
    ]
}

fn websocket_routes() -> Vec<DashboardWebSocket> {
    vec![
        websocket(
            "/api/pty",
            "PTY byte bridge for embedded Hermes TUI",
            &[4401, 4403, 1011],
            false,
        ),
        websocket(
            "/api/ws",
            "JSON-RPC metadata sidecar for model/session/sidebar controls",
            &[4401, 4403],
            false,
        ),
        websocket(
            "/api/pub",
            "PTY-side TUI gateway event publisher",
            &[4400, 4401, 4403],
            true,
        ),
        websocket(
            "/api/events",
            "Dashboard sidebar event subscriber",
            &[4400, 4401, 4403],
            true,
        ),
    ]
}

fn route(method: &str, path: &str, group: &str) -> DashboardRoute {
    let auth = if path.starts_with("/api/plugins/") {
        "plugin_exempt"
    } else if path.starts_with("/api/") {
        if is_public_api_path(path) {
            "public"
        } else {
            "session_token"
        }
    } else {
        "public_plugin_asset"
    };
    DashboardRoute {
        method: method.to_string(),
        path: path.to_string(),
        group: group.to_string(),
        auth: auth.to_string(),
    }
}

fn websocket(
    path: &str,
    purpose: &str,
    close_codes: &[u16],
    channel_required: bool,
) -> DashboardWebSocket {
    DashboardWebSocket {
        path: path.to_string(),
        purpose: purpose.to_string(),
        auth: "query_token_and_loopback".to_string(),
        enabled_flag: "_DASHBOARD_EMBEDDED_CHAT_ENABLED".to_string(),
        close_codes: close_codes.to_vec(),
        channel_required,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

const PUBLIC_API_PATHS: &[&str] = &[
    "/api/status",
    "/api/config/defaults",
    "/api/config/schema",
    "/api/model/info",
    "/api/dashboard/themes",
    "/api/dashboard/plugins",
    "/api/dashboard/plugins/rescan",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_shape_normalizes_fastapi_parameters() {
        assert_eq!(
            route_shape("/api/sessions/{session_id}/messages"),
            "/api/sessions/{}/messages"
        );
        assert_eq!(
            route_shape("/dashboard-plugins/{plugin_name}/{file_path:path}"),
            "/dashboard-plugins/{}/{}"
        );
    }

    #[test]
    fn all_builtin_api_routes_have_auth_classification() {
        for route in http_routes() {
            if PUBLIC_API_PATHS.contains(&route.path.as_str()) {
                assert_eq!(route.auth, "public");
            } else if route.path.starts_with("/api/") {
                assert_eq!(route.auth, "session_token");
            }
        }
    }

    #[test]
    fn embedded_chat_keeps_pty_as_primary_surface() {
        let chat = dashboard_snapshot().embedded_chat;
        assert_eq!(chat.pty_websocket, "/api/pty");
        assert!(chat.primary_surface.contains("hermes --tui"));
        assert!(chat
            .prohibited_primary_chat_paths
            .iter()
            .any(|p| p.contains("prompt.submit")));
    }
}

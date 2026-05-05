use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GatewayStatus {
    pub pids: Vec<u32>,
    pub runtime_health_lines: Vec<String>,
}

pub fn gateway_status(hermes_home: &Path) -> GatewayStatus {
    GatewayStatus {
        pids: gateway_pids(hermes_home),
        runtime_health_lines: runtime_health_lines(hermes_home),
    }
}

pub fn render_gateway_status(status: &GatewayStatus) -> String {
    let mut output = String::new();
    if status.pids.is_empty() {
        output.push_str("✗ Gateway is not running\n");
        append_runtime_health(&mut output, &status.runtime_health_lines);
        output.push('\n');
        output.push_str("To start:\n");
        output.push_str("  hermes gateway run      # Run in foreground\n");
        output.push_str("  hermes gateway install  # Install as user service\n");
        output.push_str(
            "  sudo hermes gateway install --system  # Install as boot-time system service\n",
        );
    } else {
        let pids = status
            .pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("✓ Gateway is running (PID: {pids})\n"));
        output.push_str("  (Running manually, not as a system service)\n");
        append_runtime_health(&mut output, &status.runtime_health_lines);
        output.push('\n');
        output.push_str("To install as a service:\n");
        output.push_str("  hermes gateway install\n");
        output.push_str("  sudo hermes gateway install --system\n");
    }
    output
}

fn append_runtime_health(output: &mut String, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    output.push('\n');
    output.push_str("Recent gateway health:\n");
    for line in lines {
        output.push_str("  ");
        output.push_str(line);
        output.push('\n');
    }
}

fn gateway_pids(hermes_home: &Path) -> Vec<u32> {
    let pid_path = hermes_home.join("gateway.pid");
    let lock_path = hermes_home.join("gateway.lock");
    let mut pids = Vec::new();
    for path in [pid_path, lock_path] {
        if let Some(pid) = read_pid(&path) {
            if !pids.contains(&pid) && process_exists(pid) {
                pids.push(pid);
            }
        }
    }
    pids
}

fn read_pid(path: &Path) -> Option<u32> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(pid) = trimmed.parse::<u32>() {
        return Some(pid);
    }
    let payload = serde_json::from_str::<Value>(trimmed).ok()?;
    match payload {
        Value::Number(number) => number.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::Object(map) => map
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        _ => None,
    }
}

fn process_exists(pid: u32) -> bool {
    Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn runtime_health_lines(hermes_home: &Path) -> Vec<String> {
    let path = hermes_home.join("gateway_state.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(state) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(root) = state.as_object() else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    if let Some(platforms) = root.get("platforms").and_then(Value::as_object) {
        for (platform, data) in platforms {
            if data.get("state").and_then(Value::as_str) == Some("fatal") {
                let message = data
                    .get("error_message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                lines.push(format!("⚠ {platform}: {message}"));
            }
        }
    }

    let gateway_state = root.get("gateway_state").and_then(Value::as_str);
    let exit_reason = root.get("exit_reason").and_then(Value::as_str);
    let restart_requested = root
        .get("restart_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_agents = root
        .get("active_agents")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    match (gateway_state, exit_reason) {
        (Some("startup_failed"), Some(reason)) if !reason.is_empty() => {
            lines.push(format!("⚠ Last startup issue: {reason}"));
        }
        (Some("draining"), _) => {
            let action = if restart_requested {
                "restart"
            } else {
                "shutdown"
            };
            lines.push(format!(
                "⏳ Gateway draining for {action} ({active_agents} active agent(s))"
            ));
        }
        (Some("stopped"), Some(reason)) if !reason.is_empty() => {
            lines.push(format!("⚠ Last shutdown reason: {reason}"));
        }
        _ => {}
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_not_running_status() {
        let rendered = render_gateway_status(&GatewayStatus {
            pids: vec![],
            runtime_health_lines: vec![],
        });
        assert!(rendered.contains("Gateway is not running"));
        assert!(rendered.contains("hermes gateway run"));
    }
}

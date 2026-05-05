use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::gateway_status::gateway_status;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CronStatus {
    pub gateway_pids: Vec<u32>,
    pub active_jobs: usize,
    pub next_run: Option<String>,
}

pub fn cron_status(hermes_home: &Path) -> CronStatus {
    let jobs = active_cron_jobs(hermes_home);
    let next_run = jobs
        .iter()
        .filter_map(|job| {
            job.get("next_run_at")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .min();
    CronStatus {
        gateway_pids: gateway_status(hermes_home).pids,
        active_jobs: jobs.len(),
        next_run,
    }
}

pub fn render_cron_status(status: &CronStatus) -> String {
    let mut output = String::new();
    output.push('\n');
    if status.gateway_pids.is_empty() {
        output.push_str("✗ Gateway is not running — cron jobs will NOT fire\n\n");
        output.push_str("  To enable automatic execution:\n");
        output.push_str("    hermes gateway install    # Install as a user service\n");
        output.push_str(
            "    sudo hermes gateway install --system  # Linux servers: boot-time system service\n",
        );
        output.push_str("    hermes gateway            # Or run in foreground\n");
    } else {
        output.push_str("✓ Gateway is running — cron jobs will fire automatically\n");
        output.push_str(&format!(
            "  PID: {}\n",
            status
                .gateway_pids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    output.push('\n');
    if status.active_jobs == 0 {
        output.push_str("  No active jobs\n");
    } else {
        output.push_str(&format!("  {} active job(s)\n", status.active_jobs));
        if let Some(next_run) = &status.next_run {
            output.push_str(&format!("  Next run: {next_run}\n"));
        }
    }
    output.push('\n');
    output
}

fn active_cron_jobs(hermes_home: &Path) -> Vec<Value> {
    let path = hermes_home.join("cron").join("jobs.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    let Some(jobs) = root.get("jobs").and_then(Value::as_array) else {
        return Vec::new();
    };
    jobs.iter()
        .filter(|job| job.get("enabled").and_then(Value::as_bool).unwrap_or(true))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_no_gateway_no_jobs_status() {
        let rendered = render_cron_status(&CronStatus {
            gateway_pids: vec![],
            active_jobs: 0,
            next_run: None,
        });
        assert!(rendered.contains("Gateway is not running"));
        assert!(rendered.contains("No active jobs"));
    }
}

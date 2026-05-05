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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CronList {
    pub gateway_pids: Vec<u32>,
    pub jobs: Vec<Value>,
}

pub fn cron_status(hermes_home: &Path) -> CronStatus {
    let jobs = cron_jobs(hermes_home, false);
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

pub fn cron_list(hermes_home: &Path, include_disabled: bool) -> CronList {
    CronList {
        gateway_pids: gateway_status(hermes_home).pids,
        jobs: cron_jobs(hermes_home, include_disabled),
    }
}

pub fn render_cron_list(list: &CronList) -> String {
    let mut output = String::new();
    if list.jobs.is_empty() {
        output.push_str("No scheduled jobs.\n");
        output.push_str("Create one with 'hermes cron create ...' or the /cron command in chat.\n");
        return output;
    }

    output.push('\n');
    output
        .push_str("┌─────────────────────────────────────────────────────────────────────────┐\n");
    output
        .push_str("│                         Scheduled Jobs                                  │\n");
    output
        .push_str("└─────────────────────────────────────────────────────────────────────────┘\n");
    output.push('\n');

    for job in &list.jobs {
        let job_id = string_field(job, "id", "?");
        let name = string_field(job, "name", "(unnamed)");
        let schedule = schedule_display(job);
        let state = job
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if enabled(job) {
                    "scheduled".to_string()
                } else {
                    "paused".to_string()
                }
            });
        let next_run = string_field(job, "next_run_at", "?");
        let repeat = repeat_display(job);
        let deliver = deliver_display(job);
        let skills = skills_for_job(job);
        let status = status_display(&state, enabled(job));

        output.push_str(&format!("  {job_id} {status}\n"));
        output.push_str(&format!("    Name:      {name}\n"));
        output.push_str(&format!("    Schedule:  {schedule}\n"));
        output.push_str(&format!("    Repeat:    {repeat}\n"));
        output.push_str(&format!("    Next run:  {next_run}\n"));
        output.push_str(&format!("    Deliver:   {deliver}\n"));
        if !skills.is_empty() {
            output.push_str(&format!("    Skills:    {}\n", skills.join(", ")));
        }
        if let Some(script) = non_empty_string(job, "script") {
            output.push_str(&format!("    Script:    {script}\n"));
        }
        if let Some(workdir) = non_empty_string(job, "workdir") {
            output.push_str(&format!("    Workdir:   {workdir}\n"));
        }
        if let Some(last_status) = non_empty_string(job, "last_status") {
            let last_run = string_field(job, "last_run_at", "?");
            let status_text = if last_status == "ok" {
                "ok".to_string()
            } else {
                format!("{}: {}", last_status, string_field(job, "last_error", "?"))
            };
            output.push_str(&format!("    Last run:  {last_run}  {status_text}\n"));
        }
        if let Some(delivery_error) = non_empty_string(job, "last_delivery_error") {
            output.push_str(&format!("    ⚠ Delivery failed: {delivery_error}\n"));
        }
        output.push('\n');
    }

    if list.gateway_pids.is_empty() {
        output.push_str("  ⚠  Gateway is not running — jobs won't fire automatically.\n");
        output.push_str("     Start it with: hermes gateway install\n");
        output.push_str(
            "                    sudo hermes gateway install --system  # Linux servers\n",
        );
        output.push('\n');
    }

    output
}

fn cron_jobs(hermes_home: &Path, include_disabled: bool) -> Vec<Value> {
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
        .filter_map(|job| {
            let mut normalized = job.clone();
            normalize_skill_fields(&mut normalized);
            if include_disabled || enabled(&normalized) {
                Some(normalized)
            } else {
                None
            }
        })
        .collect()
}

fn normalize_skill_fields(job: &mut Value) {
    let Some(map) = job.as_object_mut() else {
        return;
    };

    let mut skills = Vec::new();
    if let Some(value) = map.get("skills").filter(|value| !value.is_null()) {
        match value {
            Value::String(skill) => push_unique_skill(&mut skills, skill),
            Value::Array(items) => {
                for item in items {
                    if let Some(skill) = item.as_str() {
                        push_unique_skill(&mut skills, skill);
                    }
                }
            }
            _ => {}
        }
    } else if let Some(skill) = map.get("skill").and_then(Value::as_str) {
        push_unique_skill(&mut skills, skill);
    }

    if !skills.is_empty() || map.contains_key("skills") || map.contains_key("skill") {
        map.insert(
            "skills".to_string(),
            Value::Array(skills.iter().cloned().map(Value::String).collect()),
        );
        map.insert(
            "skill".to_string(),
            skills
                .first()
                .cloned()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
    }
}

fn push_unique_skill(skills: &mut Vec<String>, skill: &str) {
    let trimmed = skill.trim();
    if !trimmed.is_empty() && !skills.iter().any(|existing| existing == trimmed) {
        skills.push(trimmed.to_string());
    }
}

fn string_field(job: &Value, key: &str, default: &str) -> String {
    match job.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Null) | None => default.to_string(),
        Some(value) => value_to_pythonish_string(value),
    }
}

fn non_empty_string(job: &Value, key: &str) -> Option<String> {
    let value = string_field(job, key, "");
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn value_to_pythonish_string(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => {
            let parts = items
                .iter()
                .map(value_to_pythonish_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{parts}]")
        }
        Value::Object(_) => value.to_string(),
    }
}

fn schedule_display(job: &Value) -> String {
    if let Some(display) = job.get("schedule_display").and_then(Value::as_str) {
        return display.to_string();
    }
    job.get("schedule")
        .and_then(|schedule| schedule.get("value"))
        .map(value_to_pythonish_string)
        .unwrap_or_else(|| "?".to_string())
}

fn enabled(job: &Value) -> bool {
    job.get("enabled").and_then(Value::as_bool).unwrap_or(true)
}

fn repeat_display(job: &Value) -> String {
    let repeat = job.get("repeat").unwrap_or(&Value::Null);
    let times = repeat.get("times").and_then(Value::as_i64);
    match times {
        Some(value) if value != 0 => {
            let completed = repeat.get("completed").and_then(Value::as_i64).unwrap_or(0);
            format!("{completed}/{value}")
        }
        _ => "∞".to_string(),
    }
}

fn deliver_display(job: &Value) -> String {
    match job.get("deliver") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(value_to_pythonish_string)
            .collect::<Vec<_>>()
            .join(", "),
        Some(value) => value_to_pythonish_string(value),
        None => "local".to_string(),
    }
}

fn skills_for_job(job: &Value) -> Vec<String> {
    if let Some(items) = job.get("skills").and_then(Value::as_array) {
        return items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    job.get("skill")
        .and_then(Value::as_str)
        .filter(|skill| !skill.is_empty())
        .map(|skill| vec![skill.to_string()])
        .unwrap_or_default()
}

fn status_display(state: &str, enabled: bool) -> &'static str {
    if state == "paused" {
        "[paused]"
    } else if state == "completed" {
        "[completed]"
    } else if enabled {
        "[active]"
    } else {
        "[disabled]"
    }
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

    #[test]
    fn renders_cron_list_jobs() {
        let rendered = render_cron_list(&CronList {
            gateway_pids: vec![],
            jobs: vec![serde_json::json!({
                "id": "job-a",
                "name": "Alpha",
                "schedule_display": "every 5 minutes",
                "enabled": true,
                "next_run_at": "2026-05-06T09:00:00+00:00",
                "repeat": {"times": 3, "completed": 1},
                "deliver": ["local", "telegram"],
                "skills": ["foo", "bar"],
            })],
        });
        assert!(rendered.contains("Scheduled Jobs"));
        assert!(rendered.contains("job-a [active]"));
        assert!(rendered.contains("Repeat:    1/3"));
        assert!(rendered.contains("Skills:    foo, bar"));
        assert!(rendered.contains("Gateway is not running"));
    }
}

use regex::Regex;
use serde_json::{json, Value};

const FIXED_NOW: &str = "2026-05-05T12:00:00+00:00";

#[derive(Clone, Debug, Default)]
pub struct CronJobRequest {
    pub action: String,
    pub job_id: Option<String>,
    pub prompt: Option<String>,
    pub schedule: Option<String>,
    pub name: Option<String>,
    pub repeat: Option<i64>,
    pub deliver: Option<Value>,
    pub include_disabled: bool,
    pub skill: Option<String>,
    pub skills: Option<Value>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CronJobStore {
    jobs: Vec<CronJob>,
    next_id: String,
}

impl CronJobStore {
    pub fn fixture() -> Self {
        Self {
            jobs: Vec::new(),
            next_id: "abc123abc123".to_string(),
        }
    }

    fn create_id(&self) -> String {
        self.next_id.clone()
    }

    fn find(&self, job_id: &str) -> Option<&CronJob> {
        self.jobs.iter().find(|job| job.id == job_id)
    }

    fn find_mut(&mut self, job_id: &str) -> Option<&mut CronJob> {
        self.jobs.iter_mut().find(|job| job.id == job_id)
    }
}

#[derive(Clone, Debug)]
struct CronJob {
    id: String,
    name: String,
    prompt: String,
    skills: Vec<String>,
    model: Option<String>,
    provider: Option<String>,
    base_url: Option<String>,
    schedule_display: String,
    schedule_minutes: i64,
    repeat_times: Option<i64>,
    repeat_completed: i64,
    deliver: String,
    next_run_at: Option<String>,
    last_run_at: Option<String>,
    last_status: Option<String>,
    last_delivery_error: Option<String>,
    enabled: bool,
    state: String,
    paused_at: Option<String>,
    paused_reason: Option<String>,
}

pub fn cronjob_response(store: &mut CronJobStore, request: CronJobRequest) -> Value {
    let normalized = request.action.trim().to_ascii_lowercase();
    if normalized == "create" {
        return create_job(store, request);
    }
    if normalized == "list" {
        let jobs = store
            .jobs
            .iter()
            .filter(|job| request.include_disabled || job.enabled)
            .map(format_job)
            .collect::<Vec<_>>();
        return json!({"success": true, "count": jobs.len(), "jobs": jobs});
    }

    let Some(job_id) = request.job_id.clone() else {
        return tool_error(&format!("job_id is required for action '{normalized}'"));
    };
    let Some(existing) = store.find(&job_id).cloned() else {
        return json!({
            "success": false,
            "error": format!("Job with ID '{job_id}' not found. Use cronjob(action='list') to inspect jobs."),
        });
    };

    match normalized.as_str() {
        "remove" => {
            store.jobs.retain(|job| job.id != job_id);
            json!({
                "success": true,
                "message": format!("Cron job '{}' removed.", existing.name),
                "removed_job": {
                    "id": job_id,
                    "name": existing.name,
                    "schedule": existing.schedule_display,
                },
            })
        }
        "pause" => {
            let Some(job) = store.find_mut(&job_id) else {
                return tool_error(&format!("Failed to pause job '{job_id}'"));
            };
            job.enabled = false;
            job.state = "paused".to_string();
            job.paused_at = Some(FIXED_NOW.to_string());
            job.paused_reason = request.reason.and_then(normalize_optional);
            json!({"success": true, "job": format_job(job)})
        }
        "resume" => {
            let Some(job) = store.find_mut(&job_id) else {
                return tool_error(&format!("Failed to resume job '{job_id}'"));
            };
            job.enabled = true;
            job.state = "scheduled".to_string();
            job.paused_at = None;
            job.paused_reason = None;
            job.next_run_at = Some(next_run_at(job.schedule_minutes));
            json!({"success": true, "job": format_job(job)})
        }
        "run" | "run_now" | "trigger" => {
            let Some(job) = store.find_mut(&job_id) else {
                return tool_error(&format!("Failed to trigger job '{job_id}'"));
            };
            job.enabled = true;
            job.state = "scheduled".to_string();
            job.paused_at = None;
            job.paused_reason = None;
            job.next_run_at = Some(FIXED_NOW.to_string());
            json!({"success": true, "job": format_job(job)})
        }
        "update" => update_job(store, &job_id, request),
        _ => tool_error(&format!("Unknown cron action '{}'", request.action)),
    }
}

pub fn scan_cron_prompt(prompt: &str) -> String {
    for ch in [
        '\u{200b}', '\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}', '\u{202a}', '\u{202b}',
        '\u{202c}', '\u{202d}', '\u{202e}',
    ] {
        if prompt.contains(ch) {
            return format!(
                "Blocked: prompt contains invisible unicode U+{:04X} (possible injection).",
                ch as u32
            );
        }
    }

    for (pattern, id) in threat_patterns() {
        let regex = Regex::new(&format!("(?i){pattern}")).expect("valid cron threat pattern");
        if regex.is_match(prompt) {
            return format!(
                "Blocked: prompt matches threat pattern '{id}'. Cron prompts must not contain injection or exfiltration payloads."
            );
        }
    }
    String::new()
}

fn create_job(store: &mut CronJobStore, request: CronJobRequest) -> Value {
    let Some(schedule) = request
        .schedule
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        return tool_error("schedule is required for create");
    };
    let skills = canonical_skills(request.skill.as_deref(), request.skills.as_ref());
    if request.prompt.as_deref().unwrap_or("").is_empty() && skills.is_empty() {
        return tool_error("create requires either prompt or at least one skill");
    }
    if let Some(prompt) = request.prompt.as_deref() {
        let scan_error = scan_cron_prompt(prompt);
        if !scan_error.is_empty() {
            return tool_error(&scan_error);
        }
    }

    let parsed = match parse_interval_schedule(schedule) {
        Ok(parsed) => parsed,
        Err(error) => return tool_error(&error),
    };
    let repeat_times = normalize_repeat_for_create(request.repeat, &parsed.kind);
    let prompt = request.prompt.unwrap_or_default();
    let label_source = if prompt.is_empty() {
        skills
            .first()
            .cloned()
            .unwrap_or_else(|| "cron job".to_string())
    } else {
        prompt.clone()
    };
    let name = request
        .name
        .and_then(normalize_optional)
        .unwrap_or_else(|| {
            label_source
                .chars()
                .take(50)
                .collect::<String>()
                .trim()
                .to_string()
        });
    let deliver =
        normalize_deliver_param(request.deliver.as_ref()).unwrap_or_else(|| "local".to_string());
    let job = CronJob {
        id: store.create_id(),
        name,
        prompt,
        skills,
        model: request.model.and_then(normalize_optional),
        provider: request.provider.and_then(normalize_optional),
        base_url: request.base_url.and_then(normalize_base_url),
        schedule_display: parsed.display,
        schedule_minutes: parsed.minutes,
        repeat_times,
        repeat_completed: 0,
        deliver,
        next_run_at: Some(next_run_at(parsed.minutes)),
        last_run_at: None,
        last_status: None,
        last_delivery_error: None,
        enabled: true,
        state: "scheduled".to_string(),
        paused_at: None,
        paused_reason: None,
    };
    let formatted = format_job(&job);
    let response = json!({
        "success": true,
        "job_id": job.id,
        "name": job.name,
        "skill": job.skills.first().cloned(),
        "skills": job.skills,
        "schedule": job.schedule_display,
        "repeat": repeat_display(&job),
        "deliver": job.deliver,
        "next_run_at": job.next_run_at,
        "job": formatted,
        "message": format!("Cron job '{}' created.", job.name),
    });
    store.jobs.push(job);
    response
}

fn update_job(store: &mut CronJobStore, job_id: &str, request: CronJobRequest) -> Value {
    let Some(job) = store.find_mut(job_id) else {
        return tool_error(&format!("Job with ID '{job_id}' not found"));
    };
    let mut changed = false;

    if let Some(prompt) = request.prompt {
        let scan_error = scan_cron_prompt(&prompt);
        if !scan_error.is_empty() {
            return tool_error(&scan_error);
        }
        job.prompt = prompt;
        changed = true;
    }
    if let Some(name) = request.name {
        job.name = name;
        changed = true;
    }
    if let Some(deliver) = request.deliver.as_ref() {
        job.deliver = normalize_deliver_param(Some(deliver)).unwrap_or_default();
        changed = true;
    }
    if request.skills.is_some() || request.skill.is_some() {
        job.skills = canonical_skills(request.skill.as_deref(), request.skills.as_ref());
        changed = true;
    }
    if let Some(model) = request.model {
        job.model = normalize_optional(model);
        changed = true;
    }
    if let Some(provider) = request.provider {
        job.provider = normalize_optional(provider);
        changed = true;
    }
    if let Some(base_url) = request.base_url {
        job.base_url = normalize_base_url(base_url);
        changed = true;
    }
    if let Some(repeat) = request.repeat {
        job.repeat_times = if repeat <= 0 { None } else { Some(repeat) };
        changed = true;
    }
    if let Some(schedule) = request.schedule {
        let parsed = match parse_interval_schedule(&schedule) {
            Ok(parsed) => parsed,
            Err(error) => return tool_error(&error),
        };
        job.schedule_display = parsed.display;
        job.schedule_minutes = parsed.minutes;
        if job.state != "paused" {
            job.state = "scheduled".to_string();
            job.enabled = true;
            job.next_run_at = Some(next_run_at(parsed.minutes));
        }
        changed = true;
    }
    if !changed {
        return tool_error("No updates provided.");
    }
    json!({"success": true, "job": format_job(job)})
}

fn format_job(job: &CronJob) -> Value {
    json!({
        "job_id": job.id,
        "name": job.name,
        "skill": job.skills.first().cloned(),
        "skills": job.skills,
        "prompt_preview": prompt_preview(&job.prompt),
        "model": job.model,
        "provider": job.provider,
        "base_url": job.base_url,
        "schedule": job.schedule_display,
        "repeat": repeat_display(job),
        "deliver": job.deliver,
        "next_run_at": job.next_run_at,
        "last_run_at": job.last_run_at,
        "last_status": job.last_status,
        "last_delivery_error": job.last_delivery_error,
        "enabled": job.enabled,
        "state": job.state,
        "paused_at": job.paused_at,
        "paused_reason": job.paused_reason,
    })
}

fn prompt_preview(prompt: &str) -> String {
    let count = prompt.chars().count();
    if count > 100 {
        format!("{}...", prompt.chars().take(100).collect::<String>())
    } else {
        prompt.to_string()
    }
}

fn repeat_display(job: &CronJob) -> String {
    match job.repeat_times {
        None => "forever".to_string(),
        Some(1) if job.repeat_completed == 0 => "once".to_string(),
        Some(1) => "1/1".to_string(),
        Some(times) if job.repeat_completed > 0 => format!("{}/{times}", job.repeat_completed),
        Some(times) => format!("{times} times"),
    }
}

fn canonical_skills(skill: Option<&str>, skills: Option<&Value>) -> Vec<String> {
    let raw_items = match skills {
        None => skill.into_iter().map(str::to_string).collect::<Vec<_>>(),
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(items)) => items.iter().map(value_to_text).collect::<Vec<_>>(),
        Some(value) => vec![value_to_text(value)],
    };
    let mut normalized = Vec::new();
    for item in raw_items {
        let text = item.trim().to_string();
        if !text.is_empty() && !normalized.contains(&text) {
            normalized.push(text);
        }
    }
    normalized
}

fn normalize_deliver_param(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Array(items)) => {
            let parts = items
                .iter()
                .map(value_to_text)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(","))
            }
        }
        Some(value) => normalize_optional(value_to_text(value)),
    }
}

fn normalize_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_base_url(value: String) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_repeat_for_create(repeat: Option<i64>, kind: &str) -> Option<i64> {
    match repeat {
        Some(value) if value <= 0 => None,
        Some(value) => Some(value),
        None if kind == "once" => Some(1),
        None => None,
    }
}

#[derive(Clone, Debug)]
struct ParsedSchedule {
    kind: String,
    minutes: i64,
    display: String,
}

fn parse_interval_schedule(schedule: &str) -> Result<ParsedSchedule, String> {
    let schedule = schedule.trim();
    let lower = schedule.to_ascii_lowercase();
    if let Some(duration) = lower.strip_prefix("every ") {
        let minutes = parse_duration(duration.trim())?;
        return Ok(ParsedSchedule {
            kind: "interval".to_string(),
            minutes,
            display: format!("every {minutes}m"),
        });
    }
    let minutes = parse_duration(&lower)?;
    Ok(ParsedSchedule {
        kind: "once".to_string(),
        minutes,
        display: format!("once in {schedule}"),
    })
}

fn parse_duration(value: &str) -> Result<i64, String> {
    let regex = Regex::new(r"^(\d+)\s*(m|min|mins|minute|minutes|h|hr|hrs|hour|hours|d|day|days)$")
        .expect("valid duration regex");
    let Some(captures) = regex.captures(value) else {
        return Err(format!(
            "Invalid duration: '{}'. Use format like '30m', '2h', or '1d'",
            value
        ));
    };
    let amount = captures
        .get(1)
        .and_then(|m| m.as_str().parse::<i64>().ok())
        .unwrap_or(0);
    let unit = captures
        .get(2)
        .and_then(|m| m.as_str().chars().next())
        .unwrap_or('m');
    Ok(amount
        * match unit {
            'h' => 60,
            'd' => 1440,
            _ => 1,
        })
}

fn next_run_at(minutes: i64) -> String {
    let total_minutes = 12 * 60 + minutes;
    let hour = total_minutes / 60;
    let minute = total_minutes % 60;
    format!("2026-05-05T{hour:02}:{minute:02}:00+00:00")
}

fn value_to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(value) => {
            if *value {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Number(number) => number.to_string(),
        other => other.to_string(),
    }
}

fn tool_error(error: &str) -> Value {
    json!({"error": error, "success": false})
}

fn threat_patterns() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            r"ignore\s+(?:\w+\s+)*(?:previous|all|above|prior)\s+(?:\w+\s+)*instructions",
            "prompt_injection",
        ),
        (r"do\s+not\s+tell\s+the\s+user", "deception_hide"),
        (r"system\s+prompt\s+override", "sys_prompt_override"),
        (
            r"disregard\s+(your|all|any)\s+(instructions|rules|guidelines)",
            "disregard_rules",
        ),
        (
            r"curl\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|API)",
            "exfil_curl",
        ),
        (
            r"wget\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|API)",
            "exfil_wget",
        ),
        (
            r"cat\s+[^\n]*(\.env|credentials|\.netrc|\.pgpass)",
            "read_secrets",
        ),
        (r"authorized_keys", "ssh_backdoor"),
        (r"/etc/sudoers|visudo", "sudoers_mod"),
        (r"rm\s+-rf\s+/", "destructive_root_rm"),
    ]
}

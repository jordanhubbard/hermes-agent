use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDateTime, TimeZone};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogsOutcome {
    pub output: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LogsRequest {
    log_name: String,
    lines: usize,
    follow: bool,
    level: Option<String>,
    session: Option<String>,
    component: Option<String>,
    since: Option<String>,
}

pub fn run_logs_command(args: &[OsString], hermes_home: &Path, display_home: &str) -> LogsOutcome {
    let request = match parse_logs_args(args) {
        Ok(request) => request,
        Err(message) => {
            return LogsOutcome {
                output: format!("{message}\n"),
                exit_code: 2,
            };
        }
    };

    if request.follow {
        return LogsOutcome {
            output: "HERMES_RUNTIME=rust selected, but logs follow mode is not Rust-owned yet. Use HERMES_RUNTIME=python for the rollout fallback.\n".to_string(),
            exit_code: 78,
        };
    }

    if request.log_name == "list" {
        return LogsOutcome {
            output: render_log_list(hermes_home, display_home),
            exit_code: 0,
        };
    }

    match render_log_tail(&request, hermes_home, display_home) {
        Ok(output) => LogsOutcome {
            output,
            exit_code: 0,
        },
        Err(message) => LogsOutcome {
            output: format!("{message}\n"),
            exit_code: 1,
        },
    }
}

fn parse_logs_args(args: &[OsString]) -> Result<LogsRequest, String> {
    let mut request = LogsRequest {
        log_name: "agent".to_string(),
        lines: 50,
        follow: false,
        level: None,
        session: None,
        component: None,
        since: None,
    };
    let mut positional = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("-f") || arg == OsStr::new("--follow") {
            request.follow = true;
            i += 1;
            continue;
        }
        if arg == OsStr::new("-n") || arg == OsStr::new("--lines") {
            let Some(value) = args.get(i + 1) else {
                return Err("argument --lines requires a value".to_string());
            };
            request.lines = value
                .to_string_lossy()
                .parse::<usize>()
                .map_err(|_| format!("invalid --lines value: {}", value.to_string_lossy()))?;
            i += 2;
            continue;
        }
        if let Some(value) = arg.to_string_lossy().strip_prefix("--lines=") {
            request.lines = value
                .parse::<usize>()
                .map_err(|_| format!("invalid --lines value: {value}"))?;
            i += 1;
            continue;
        }
        if arg == OsStr::new("--level") {
            let Some(value) = args.get(i + 1) else {
                return Err("argument --level requires a value".to_string());
            };
            request.level = Some(value.to_string_lossy().into_owned());
            i += 2;
            continue;
        }
        if arg == OsStr::new("--session") {
            let Some(value) = args.get(i + 1) else {
                return Err("argument --session requires a value".to_string());
            };
            request.session = Some(value.to_string_lossy().into_owned());
            i += 2;
            continue;
        }
        if arg == OsStr::new("--component") {
            let Some(value) = args.get(i + 1) else {
                return Err("argument --component requires a value".to_string());
            };
            request.component = Some(value.to_string_lossy().into_owned());
            i += 2;
            continue;
        }
        if arg == OsStr::new("--since") {
            let Some(value) = args.get(i + 1) else {
                return Err("argument --since requires a value".to_string());
            };
            request.since = Some(value.to_string_lossy().into_owned());
            i += 2;
            continue;
        }
        if let Some(value) = arg.to_string_lossy().strip_prefix("--since=") {
            request.since = Some(value.to_string());
            i += 1;
            continue;
        }
        if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown logs option: {}", arg.to_string_lossy()));
        }
        positional.push(arg.to_string_lossy().into_owned());
        i += 1;
    }
    if let Some(log_name) = positional.first() {
        request.log_name = log_name.clone();
    }
    Ok(request)
}

fn render_log_list(hermes_home: &Path, display_home: &str) -> String {
    let log_dir = hermes_home.join("logs");
    if !log_dir.exists() {
        return format!("No logs directory at {display_home}/logs/\n");
    }

    let mut output = format!("Log files in {display_home}/logs/:\n\n");
    let mut found = false;
    let Ok(entries) = fs::read_dir(log_dir) else {
        output.push_str("  (no log files yet — run 'hermes chat' to generate logs)\n");
        return output;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if !path.is_file() || path.extension() != Some(OsStr::new("log")) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let size = meta.len();
        let size_str = if size < 1024 {
            format!("{size}B")
        } else if size < 1024 * 1024 {
            format!("{:.1}KB", size as f64 / 1024.0)
        } else {
            format!("{:.1}MB", size as f64 / (1024.0 * 1024.0))
        };
        let age = meta
            .modified()
            .ok()
            .and_then(|mtime| SystemTime::now().duration_since(mtime).ok())
            .unwrap_or(Duration::ZERO);
        let age_str = if age.as_secs() < 60 {
            "just now".to_string()
        } else if age.as_secs() < 3600 {
            format!("{}m ago", age.as_secs() / 60)
        } else if age.as_secs() < 86400 {
            format!("{}h ago", age.as_secs() / 3600)
        } else {
            match meta.modified() {
                Ok(mtime) => format_days_old(mtime),
                Err(_) => "unknown".to_string(),
            }
        };
        output.push_str(&format!(
            "  {:<25} {:>8}   {age_str}\n",
            entry.file_name().to_string_lossy(),
            size_str
        ));
        found = true;
    }
    if !found {
        output.push_str("  (no log files yet — run 'hermes chat' to generate logs)\n");
    }
    output
}

fn render_log_tail(
    request: &LogsRequest,
    hermes_home: &Path,
    display_home: &str,
) -> Result<String, String> {
    let Some(filename) = log_filename(&request.log_name) else {
        return Err(format!(
            "Unknown log: {:?}. Available: agent, errors, gateway",
            request.log_name
        ));
    };
    let log_path = hermes_home.join("logs").join(filename);
    if !log_path.exists() {
        return Err(format!(
            "Log file not found: {}\n(Logs are created when Hermes runs — try 'hermes chat' first)",
            log_path.display()
        ));
    }

    let min_level = match request.level.as_ref() {
        Some(level) => {
            let upper = level.to_ascii_uppercase();
            if level_rank(&upper).is_none() {
                return Err(format!(
                    "Invalid --level: {level:?}. Use DEBUG, INFO, WARNING, ERROR, or CRITICAL."
                ));
            }
            Some(upper)
        }
        None => None,
    };
    let component_prefixes = match request.component.as_ref() {
        Some(component) => Some(component_prefixes(component)?),
        None => None,
    };
    let since_cutoff = match request.since.as_ref() {
        Some(since) => Some(parse_since_cutoff(since)?),
        None => None,
    };
    let has_filters = min_level.is_some()
        || request.session.is_some()
        || component_prefixes.is_some()
        || since_cutoff.is_some();
    let raw_lines = read_last_lines(
        &log_path,
        if has_filters {
            request.lines.saturating_mul(20).max(2000)
        } else {
            request.lines
        },
    )?;
    let lines = if has_filters {
        raw_lines
            .into_iter()
            .filter(|line| {
                matches_filters(
                    line,
                    min_level.as_deref(),
                    request.session.as_deref(),
                    component_prefixes.as_deref(),
                    since_cutoff,
                )
            })
            .rev()
            .take(request.lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
    } else {
        raw_lines
    };

    let mut filter_parts = Vec::new();
    if let Some(level) = &min_level {
        filter_parts.push(format!("level>={level}"));
    }
    if let Some(session) = &request.session {
        filter_parts.push(format!("session={session}"));
    }
    if let Some(component) = &request.component {
        filter_parts.push(format!("component={component}"));
    }
    if let Some(since) = &request.since {
        filter_parts.push(format!("since={since}"));
    }
    let filter_desc = if filter_parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", filter_parts.join(", "))
    };

    let mut output = format!(
        "--- {display_home}/logs/{filename}{filter_desc} (last {}) ---\n",
        request.lines
    );
    for line in lines {
        output.push_str(&line);
        if !line.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

fn log_filename(log_name: &str) -> Option<&'static str> {
    match log_name {
        "agent" => Some("agent.log"),
        "errors" => Some("errors.log"),
        "gateway" => Some("gateway.log"),
        _ => None,
    }
}

fn read_last_lines(path: &Path, n: usize) -> Result<Vec<String>, String> {
    let raw = fs::read_to_string(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            format!("Permission denied: {}", path.display())
        } else {
            err.to_string()
        }
    })?;
    let mut lines = raw
        .split_inclusive('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if !raw.ends_with('\n') && !raw.is_empty() && lines.is_empty() {
        lines.push(raw);
    }
    let start = lines.len().saturating_sub(n);
    Ok(lines.split_off(start))
}

fn matches_filters(
    line: &str,
    min_level: Option<&str>,
    session: Option<&str>,
    component_prefixes: Option<&[&str]>,
    since_cutoff: Option<i64>,
) -> bool {
    if let Some(cutoff) = since_cutoff {
        if let Some(ts) = parse_line_timestamp(line) {
            if ts < cutoff {
                return false;
            }
        }
    }
    if let Some(min_level) = min_level {
        if let Some(level) = extract_level(line) {
            if level_rank(level).unwrap_or(0) < level_rank(min_level).unwrap_or(0) {
                return false;
            }
        }
    }
    if let Some(session) = session {
        if !line.contains(session) {
            return false;
        }
    }
    if let Some(prefixes) = component_prefixes {
        let Some(logger) = extract_logger_name(line) else {
            return false;
        };
        if !prefixes.iter().any(|prefix| logger.starts_with(prefix)) {
            return false;
        }
    }
    true
}

fn parse_since_cutoff(since: &str) -> Result<i64, String> {
    let raw = since.trim().to_ascii_lowercase();
    if raw.len() < 2 {
        return Err(format!(
            "Invalid --since value: {since:?}. Use format like '1h', '30m', '2d'."
        ));
    }
    let (digits, unit) = raw.split_at(raw.len() - 1);
    let value = digits.trim().parse::<i64>().map_err(|_| {
        format!("Invalid --since value: {since:?}. Use format like '1h', '30m', '2d'.")
    })?;
    let seconds = match unit {
        "s" => value,
        "m" => value.saturating_mul(60),
        "h" => value.saturating_mul(3600),
        "d" => value.saturating_mul(86_400),
        _ => {
            return Err(format!(
                "Invalid --since value: {since:?}. Use format like '1h', '30m', '2d'."
            ));
        }
    };
    Ok(Local::now().timestamp().saturating_sub(seconds))
}

fn parse_line_timestamp(line: &str) -> Option<i64> {
    let timestamp = line.get(0..19)?;
    let parsed = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S").ok()?;
    Local
        .from_local_datetime(&parsed)
        .single()
        .or_else(|| Local.from_local_datetime(&parsed).earliest())
        .map(|dt| dt.timestamp())
}

fn extract_level(line: &str) -> Option<&str> {
    ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]
        .into_iter()
        .find(|level| line.contains(&format!(" {level} ")))
}

fn extract_logger_name(line: &str) -> Option<&str> {
    let level = extract_level(line)?;
    let after_level = line.split_once(&format!(" {level} "))?.1;
    let after_session = if after_level.starts_with('[') {
        after_level.split_once("] ")?.1
    } else {
        after_level
    };
    after_session
        .split_once(':')
        .map(|(logger, _)| logger.trim())
}

fn level_rank(level: &str) -> Option<u8> {
    match level {
        "DEBUG" => Some(0),
        "INFO" => Some(1),
        "WARNING" => Some(2),
        "ERROR" => Some(3),
        "CRITICAL" => Some(4),
        _ => None,
    }
}

fn component_prefixes(component: &str) -> Result<Vec<&'static str>, String> {
    match component.to_ascii_lowercase().as_str() {
        "gateway" => Ok(vec!["gateway"]),
        "agent" => Ok(vec!["agent", "run_agent", "model_tools", "batch_runner"]),
        "tools" => Ok(vec!["tools"]),
        "cli" => Ok(vec!["hermes_cli", "cli"]),
        "cron" => Ok(vec!["cron"]),
        other => Err(format!(
            "Unknown component: {other:?}. Available: agent, cli, cron, gateway, tools"
        )),
    }
}

fn format_days_old(mtime: SystemTime) -> String {
    let days = mtime
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
        / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(m <= 2);
    (year as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_filter_matches_python_threshold_semantics() {
        assert!(matches_filters(
            "2026-05-05 10:00:00 ERROR tools.file: bad",
            Some("WARNING"),
            None,
            None,
            None
        ));
        assert!(!matches_filters(
            "2026-05-05 10:00:00 INFO tools.file: info",
            Some("WARNING"),
            None,
            None,
            None
        ));
    }

    #[test]
    fn parses_since_values_like_python() {
        assert!(parse_since_cutoff("30m").is_ok());
        assert!(parse_since_cutoff("2 h").is_ok());
        assert!(parse_since_cutoff("bad").is_err());
    }
}

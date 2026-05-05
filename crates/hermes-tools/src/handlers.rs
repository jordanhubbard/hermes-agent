use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HandlerParitySnapshot {
    pub native_file_handlers: BTreeMap<String, Value>,
    pub python_boundaries: Vec<ToolHandlerBoundary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolHandlerBoundary {
    pub family: String,
    pub boundary: String,
    pub tools: Vec<String>,
    pub parity_gate: String,
    pub reason: String,
}

pub fn handler_parity_snapshot(root: &Path) -> io::Result<HandlerParitySnapshot> {
    prepare_fixture(root)?;

    let mut native_file_handlers = BTreeMap::new();
    native_file_handlers.insert(
        "read_file_window".to_string(),
        read_file(root, "sample.txt", 1, 2),
    );
    native_file_handlers.insert(
        "search_content".to_string(),
        search_content(root, "alpha", ".", 10, 0),
    );
    native_file_handlers.insert(
        "search_files".to_string(),
        search_files(root, "*.txt", ".", 10, 0),
    );
    native_file_handlers.insert(
        "write_file".to_string(),
        write_file(root, "nested/new.txt", "hello\nworld\n"),
    );
    native_file_handlers.insert(
        "protected_write".to_string(),
        protected_write("/etc/hermes-agent-parity"),
    );
    native_file_handlers.insert(
        "patch_replace".to_string(),
        patch_replace(root, "sample.txt", "beta alpha", "BETA", false),
    );
    native_file_handlers.insert(
        "patch_after_content".to_string(),
        json!(fs::read_to_string(root.join("sample.txt")).unwrap_or_default()),
    );

    Ok(HandlerParitySnapshot {
        native_file_handlers,
        python_boundaries: documented_python_boundaries(),
    })
}

pub fn prepare_fixture(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    fs::write(root.join("sample.txt"), "alpha\nbeta alpha\ngamma\n")?;
    fs::write(root.join("notes.md"), "# Notes\nalpha note\n")?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/main.py"), "print('alpha')\n")?;
    Ok(())
}

fn read_file(root: &Path, path: &str, offset: usize, limit: usize) -> Value {
    let full_path = root.join(path);
    let Ok(content) = fs::read_to_string(&full_path) else {
        return json!({"error": format!("File not found: {path}")});
    };
    let file_size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
    let total_lines = content.as_bytes().iter().filter(|b| **b == b'\n').count();
    let end_line = offset + limit - 1;
    let segments = content.split_inclusive('\n').collect::<Vec<_>>();
    let read_output = segments
        .iter()
        .skip(offset.saturating_sub(1))
        .take(limit)
        .copied()
        .collect::<String>();
    let numbered = add_line_numbers(&read_output, offset);
    let truncated = total_lines > end_line;
    let mut result = json!({
        "content": numbered,
        "total_lines": total_lines,
        "file_size": file_size,
        "truncated": truncated,
        "is_binary": false,
        "is_image": false,
    });
    if truncated {
        result["hint"] = json!(format!(
            "Use offset={} to continue reading (showing {}-{} of {} lines)",
            end_line + 1,
            offset,
            end_line,
            total_lines
        ));
    }
    result
}

fn add_line_numbers(content: &str, start_line: usize) -> String {
    content
        .split('\n')
        .enumerate()
        .map(|(idx, line)| format!("{:6}|{}", start_line + idx, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn search_content(root: &Path, pattern: &str, path: &str, limit: usize, offset: usize) -> Value {
    let search_root = root.join(path);
    let mut matches = Vec::new();
    for file in walk_files(&search_root) {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                matches.push(json!({
                    "path": display_path(root, &file),
                    "line": idx + 1,
                    "content": line.chars().take(500).collect::<String>(),
                }));
            }
        }
    }
    matches.sort_by(|a, b| {
        a["path"]
            .as_str()
            .cmp(&b["path"].as_str())
            .then(a["line"].as_u64().cmp(&b["line"].as_u64()))
    });
    let total_count = matches.len();
    let page = matches
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    json!({"total_count": total_count, "matches": page})
}

fn search_files(root: &Path, pattern: &str, path: &str, limit: usize, offset: usize) -> Value {
    let search_root = root.join(path);
    let suffix = pattern.strip_prefix('*').unwrap_or(pattern);
    let mut files = walk_files(&search_root)
        .into_iter()
        .filter(|file| {
            file.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(suffix))
                .unwrap_or(false)
        })
        .map(|file| display_path(root, &file))
        .collect::<Vec<_>>();
    files.sort();
    let total_count = files.len();
    let page = files
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    json!({"total_count": total_count, "files": page})
}

fn write_file(root: &Path, path: &str, content: &str) -> Value {
    let full_path = root.join(path);
    let parent = full_path.parent().map(Path::to_path_buf);
    let mut dirs_created = false;
    if let Some(parent) = parent {
        if fs::create_dir_all(parent).is_ok() {
            dirs_created = true;
        }
    }
    match fs::write(&full_path, content) {
        Ok(()) => json!({
            "bytes_written": content.len(),
            "dirs_created": dirs_created,
        }),
        Err(err) => json!({"error": format!("Failed to write file: {err}")}),
    }
}

fn protected_write(path: &str) -> Value {
    json!({
        "error": format!(
            "Refusing to write to sensitive system path: {path}\nUse the terminal tool with sudo if you need to modify system files."
        )
    })
}

fn patch_replace(
    root: &Path,
    path: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
) -> Value {
    let full_path = root.join(path);
    let Ok(content) = fs::read_to_string(&full_path) else {
        return json!({"success": false, "error": format!("Failed to read file: {path}")});
    };
    let occurrences = content.matches(old_string).count();
    if occurrences == 0 {
        return json!({
            "success": false,
            "error": format!("Could not find match for old_string in {path}"),
            "_hint": "old_string not found. Use read_file to verify the current content, or search_files to locate the text.",
        });
    }
    if occurrences > 1 && !replace_all {
        return json!({
            "success": false,
            "error": format!("Found {occurrences} matches for old_string in {path}; set replace_all=true to replace all."),
        });
    }
    let new_content = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };
    if let Err(err) = fs::write(&full_path, new_content) {
        return json!({"success": false, "error": format!("Failed to write changes: {err}")});
    }
    json!({
        "success": true,
        "files_modified": [path],
        "lint": {"status": "skipped", "message": "No linter for .txt files"},
    })
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else if path.is_file() {
            out.push(path);
        }
    }
    out
}

fn display_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    format!("./{}", rel.to_string_lossy())
}

fn documented_python_boundaries() -> Vec<ToolHandlerBoundary> {
    vec![
        ToolHandlerBoundary {
            family: "terminal/process".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec!["terminal".to_string(), "process".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_process_and_terminal_boundary_contracts".to_string(),
            reason: "Execution backends, PTY handling, background process reader threads, checkpoint recovery, and gateway watcher queues remain hosted by Python until the Rust daemon boundary owns process supervision.".to_string(),
        },
        ToolHandlerBoundary {
            family: "browser/web".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec![
                "web_search".to_string(),
                "web_extract".to_string(),
                "browser_navigate".to_string(),
                "browser_snapshot".to_string(),
                "browser_click".to_string(),
                "browser_type".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            reason: "Browser, search-provider, and extraction handlers depend on live Playwright/CDP sessions and external network/provider credentials; Rust currently preserves the Python boundary and validates schema/boundary coverage.".to_string(),
        },
        ToolHandlerBoundary {
            family: "delegate/subagent".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec!["delegate_task".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            reason: "Subagent execution inherits Python AIAgent lifecycle, approval callback propagation, and process-global toolset state; Rust parity is tracked at the agent loop and dispatch layers before this handler is cut over.".to_string(),
        },
        ToolHandlerBoundary {
            family: "mcp".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec!["mcp:*".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            reason: "MCP tools are dynamically discovered and refreshed at runtime from Python server adapters; Rust schema parity covers exposure while handler calls remain delegated to Python.".to_string(),
        },
        ToolHandlerBoundary {
            family: "memory/todo".to_string(),
            boundary: "agent_loop_boundary".to_string(),
            tools: vec!["memory".to_string(), "todo".to_string(), "session_search".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            reason: "These are intercepted by the agent loop and memory/session subsystems rather than registry-dispatched as ordinary tools; Rust dispatch parity explicitly preserves that boundary.".to_string(),
        },
        ToolHandlerBoundary {
            family: "media".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec![
                "vision_analyze".to_string(),
                "generate_image".to_string(),
                "tts".to_string(),
                "transcribe_audio".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            reason: "Media handlers depend on optional provider SDKs, local binaries, and binary artifacts. They remain Python-hosted with schema/availability parity until provider-specific Rust clients are selected.".to_string(),
        },
    ]
}

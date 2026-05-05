use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::todo::{todo_response, TodoStore};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HandlerParitySnapshot {
    pub native_file_handlers: BTreeMap<String, Value>,
    pub native_agent_handlers: BTreeMap<String, Value>,
    pub python_boundaries: Vec<ToolHandlerBoundary>,
    pub native_tools: Vec<String>,
    pub boundary_tools: Vec<String>,
    pub uncovered_core_tools: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolHandlerBoundary {
    pub family: String,
    pub boundary: String,
    pub tools: Vec<String>,
    pub parity_gate: String,
    pub deletion_blocker: bool,
    pub deletion_plan: String,
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

    let native_agent_handlers = native_agent_handler_snapshot();
    let python_boundaries = documented_python_boundaries();
    let native_tools = vec![
        "patch".to_string(),
        "read_file".to_string(),
        "search_files".to_string(),
        "todo".to_string(),
        "write_file".to_string(),
    ];
    let boundary_tools = documented_boundary_tools(&python_boundaries);
    let uncovered_core_tools = uncovered_core_tools(&native_tools, &boundary_tools);

    Ok(HandlerParitySnapshot {
        native_file_handlers,
        native_agent_handlers,
        python_boundaries,
        native_tools,
        boundary_tools,
        uncovered_core_tools,
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
            tools: vec![
                "terminal".to_string(),
                "process".to_string(),
                "execute_code".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_process_and_terminal_boundary_contracts".to_string(),
            deletion_blocker: true,
            deletion_plan: "Port local/remote process supervision to a Rust daemon or require an explicitly installed external process-host adapter before deleting in-repo Python.".to_string(),
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
                "browser_scroll".to_string(),
                "browser_back".to_string(),
                "browser_press".to_string(),
                "browser_get_images".to_string(),
                "browser_vision".to_string(),
                "browser_console".to_string(),
                "browser_cdp".to_string(),
                "browser_dialog".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Choose and implement a Rust browser/search backend or define a separately shipped browser service API before Python source removal.".to_string(),
            reason: "Browser, search-provider, and extraction handlers depend on live Playwright/CDP sessions and external network/provider credentials; Rust currently preserves the Python boundary and validates schema/boundary coverage.".to_string(),
        },
        ToolHandlerBoundary {
            family: "delegate/subagent".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec!["delegate_task".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Route delegate_task through the Rust agent runtime with explicit child-session state and approval callback propagation.".to_string(),
            reason: "Subagent execution inherits Python AIAgent lifecycle, approval callback propagation, and process-global toolset state; Rust parity is tracked at the agent loop and dispatch layers before this handler is cut over.".to_string(),
        },
        ToolHandlerBoundary {
            family: "mcp".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec!["mcp:*".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Port dynamic MCP client/server discovery to Rust or require MCP servers behind a stable external JSON-RPC tool bridge.".to_string(),
            reason: "MCP tools are dynamically discovered and refreshed at runtime from Python server adapters; Rust schema parity covers exposure while handler calls remain delegated to Python.".to_string(),
        },
        ToolHandlerBoundary {
            family: "memory/session".to_string(),
            boundary: "agent_loop_boundary".to_string(),
            tools: vec!["memory".to_string(), "session_search".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Wire Rust memory providers and session search through hermes-state/context-engine before deleting Python agent-loop interceptors.".to_string(),
            reason: "These are intercepted by the agent loop and memory/session subsystems rather than registry-dispatched as ordinary tools; Rust dispatch parity explicitly preserves that boundary.".to_string(),
        },
        ToolHandlerBoundary {
            family: "media".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec![
                "vision_analyze".to_string(),
                "image_generate".to_string(),
                "text_to_speech".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_non_file_tool_families_have_documented_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Select Rust provider clients or an external media service boundary for image, vision, and speech artifacts.".to_string(),
            reason: "Media handlers depend on optional provider SDKs, local binaries, and binary artifacts. They remain Python-hosted with schema/availability parity until provider-specific Rust clients are selected.".to_string(),
        },
        ToolHandlerBoundary {
            family: "skills".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec![
                "skills_list".to_string(),
                "skill_view".to_string(),
                "skill_manage".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_all_core_tools_are_native_or_explicit_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Port skill discovery, frontmatter parsing, install/update/audit, and prompt-cache-aware slash injection to Rust or a stable external skill service.".to_string(),
            reason: "Skill tools rely on profile-scoped skill storage, provenance, setup prompts, and optional-skill install logic that is still Python-owned.".to_string(),
        },
        ToolHandlerBoundary {
            family: "clarify".to_string(),
            boundary: "platform_interaction_boundary".to_string(),
            tools: vec!["clarify".to_string()],
            parity_gate: "tests/parity/tools/test_handlers.py::test_all_core_tools_are_native_or_explicit_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Move clarify validation plus CLI/gateway prompt callbacks into Rust platform runtimes.".to_string(),
            reason: "Clarify is a platform callback rather than a normal side-effect tool; the UI interaction path is still Python-owned in CLI and gateway runtimes.".to_string(),
        },
        ToolHandlerBoundary {
            family: "cron/messaging/homeassistant".to_string(),
            boundary: "integration_runtime_boundary".to_string(),
            tools: vec![
                "cronjob".to_string(),
                "send_message".to_string(),
                "ha_list_entities".to_string(),
                "ha_get_state".to_string(),
                "ha_list_services".to_string(),
                "ha_call_service".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_all_core_tools_are_native_or_explicit_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Port cron, gateway delivery, and Home Assistant clients to Rust integration crates or require external adapters with stable request/response contracts.".to_string(),
            reason: "These tools cross gateway/integration runtimes, credentials, network adapters, and scheduler state that are not Rust-owned yet.".to_string(),
        },
        ToolHandlerBoundary {
            family: "kanban".to_string(),
            boundary: "python_runtime_boundary".to_string(),
            tools: vec![
                "kanban_show".to_string(),
                "kanban_complete".to_string(),
                "kanban_block".to_string(),
                "kanban_heartbeat".to_string(),
                "kanban_comment".to_string(),
                "kanban_create".to_string(),
                "kanban_link".to_string(),
            ],
            parity_gate: "tests/parity/tools/test_handlers.py::test_all_core_tools_are_native_or_explicit_boundaries".to_string(),
            deletion_blocker: true,
            deletion_plan: "Port kanban_db and dispatcher worker context APIs to Rust or expose them through an external task-service boundary.".to_string(),
            reason: "Kanban tools mutate dispatcher task state and enforce worker ownership through Python kanban_db and profile config.".to_string(),
        },
    ]
}

fn native_agent_handler_snapshot() -> BTreeMap<String, Value> {
    let mut store = TodoStore::default();
    let mut snapshot = BTreeMap::new();
    snapshot.insert(
        "todo_replace".to_string(),
        todo_response(
            &mut store,
            Some(&[
                json!({"id": "a", "content": "first", "status": "pending"}),
                json!({"id": "b", "content": "second", "status": "in_progress"}),
                json!({"id": "a", "content": "first updated", "status": "bad"}),
            ]),
            false,
        ),
    );
    snapshot.insert(
        "todo_merge".to_string(),
        todo_response(
            &mut store,
            Some(&[
                json!({"id": "b", "status": "completed"}),
                json!({"id": "c", "content": "third", "status": "pending"}),
            ]),
            true,
        ),
    );
    snapshot.insert(
        "todo_read".to_string(),
        todo_response(&mut store, None, false),
    );
    snapshot.insert(
        "todo_injection".to_string(),
        json!(store.format_for_injection()),
    );
    snapshot
}

fn documented_boundary_tools(boundaries: &[ToolHandlerBoundary]) -> Vec<String> {
    let mut tools = boundaries
        .iter()
        .flat_map(|boundary| boundary.tools.iter())
        .filter(|tool| !tool.ends_with(":*"))
        .cloned()
        .collect::<Vec<_>>();
    tools.sort();
    tools.dedup();
    tools
}

fn uncovered_core_tools(native_tools: &[String], boundary_tools: &[String]) -> Vec<String> {
    let covered = native_tools
        .iter()
        .chain(boundary_tools.iter())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    crate::fixture()
        .core_tools
        .into_iter()
        .filter(|tool| !covered.contains(tool))
        .collect()
}

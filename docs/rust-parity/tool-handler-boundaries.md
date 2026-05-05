# Tool Handler Rust Boundaries

`hermes-k77.4` ports the lowest-risk file/edit handlers into `crates/hermes-tools`
and keeps side-effect-heavy handler families behind documented Python runtime
boundaries until their owning runtimes move.

Native Rust handler coverage:

- `read_file` windowing, line numbering, truncation hints, binary/image flags
- `search_files` content and filename search over local workspaces
- `write_file` directory creation and byte count reporting
- `patch` replace-mode success/error semantics at the result-envelope level
- sensitive write denial for protected system paths

Documented Python boundaries:

- `terminal/process`: execution backends, PTY handling, background readers,
  checkpoint recovery, and gateway watchers stay in Python.
- `browser/web`: live Playwright/CDP sessions and provider-backed network
  search/extraction stay in Python.
- `delegate/subagent`: subagent lifecycle, approval callback propagation, and
  process-global toolset state stay in Python.
- `mcp`: dynamically discovered server adapters stay in Python.
- `memory/todo`: agent-loop-intercepted tools stay at the agent
  boundary rather than ordinary registry dispatch.
- `media`: optional provider SDKs, local binaries, and binary artifacts stay
  in Python.

Parity gate: `tests/parity/tools/test_handlers.py`.

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
- `todo` validation, replace/merge semantics, summary counts, and
  post-compression active-task injection formatting
- `clarify` question/choice validation and result/error envelope shaping
- `memory` entry threat scanning, add/replace/remove semantics, duplicate
  handling, char-limit accounting, and frozen system-prompt snapshot behavior
- `session_search` dispatcher behavior, recent-session listing, current-lineage
  exclusion, parent-session resolution, source/model metadata selection, limit
  coercion, no-result envelopes, conversation formatting, and raw-preview
  fallback when summarization is unavailable
- `skills_list` and `skill_view` for local read-only skill discovery,
  frontmatter/tag/category parsing, linked-file discovery, linked-file reads,
  missing-file suggestions, not-found suggestions, and traversal denial
- `skill_manage` local mutation semantics for create/edit/patch/delete,
  supporting-file write/remove, absorbed-into validation, size limits, and
  path traversal denial
- Home Assistant handler validation and shaping: entity/service validation,
  blocked service domains, entity filtering by domain/area, state result
  envelopes, service-list compaction, service payload construction, and service
  response parsing

Documented deletion-blocking boundaries:

- `terminal/process`: execution backends, PTY handling, background readers,
  checkpoint recovery, `execute_code`, and gateway watchers stay in Python
  until a Rust process daemon or external process-host adapter exists.
- `browser/web`: live Playwright/CDP sessions and provider-backed network
  search/extraction stay in Python until a Rust backend or external browser
  service contract exists.
- `delegate/subagent`: subagent lifecycle, approval callback propagation, and
  process-global toolset state stay in Python until delegated turns run through
  the Rust agent runtime.
- `mcp`: dynamically discovered server adapters stay in Python.
- `memory/session`: native `session_search` still needs production wiring to
  hermes-state plus provider-backed auxiliary summarization in the Rust agent
  loop before Python agent-loop interceptors can be deleted.
- `media`: optional provider SDKs, local binaries, and binary artifacts stay in
  Python until Rust provider clients or an external media service are selected.
- `skills`: plugin skills, optional-skill hub operations, provenance telemetry,
  setup prompts, and prompt-cache-aware slash injection stay in Python-owned
  CLI/plugin runtimes until those runtimes move or get a stable external
  service boundary.
- `clarify`: only the UI callbacks stay in the Python CLI/gateway platform
  layer until those runtimes are Rust-owned.
- `cron/messaging`: scheduler state and gateway delivery/send_message clients
  stay behind Python integration runtimes.
- `homeassistant`: native handler semantics still need production Rust HTTP
  client wiring with credential/config loading.
- `kanban`: dispatcher task state and worker ownership checks stay in Python
  until `kanban_db` and worker-context APIs are Rust-owned or externalized.

Parity gate: `tests/parity/tools/test_handlers.py`. The gate fails if any core
tool from `crates/hermes-tools/src/tool_registry_snapshot.json` is neither in
the native Rust list nor in an explicit boundary with a deletion plan.

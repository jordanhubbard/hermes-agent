<!-- GENERATED FROM docs/rust-parity/status.yaml — edit the YAML and run scripts/rust_parity_status.py --write -->

# Rust parity matrix

Tracks the migration of Hermes subsystems from Python to Rust. Source of truth: `docs/rust-parity/status.yaml`.

## Constraints

**Rust crates do not link to in-repo Python.** Every Rust crate in this repo is a standalone reimplementation. It must not import or link in-repo Python code, must not be built as a Python extension whose sole purpose is to be loaded by CPython (e.g. PyO3 wheels), and must not embed CPython. Where a Python module in this repo wraps a third-party library, the Rust side reaches that library directly via a Rust crate (or its native interface) — never by calling back into the Python wrapper. This rule applies to every row in the matrix below.

## Status ladder

- `planned`
- `in_progress`
- `ported`
- `tested`
- `production_wired`
- `default`
- `deferred`

## Summary

| Status | Count | Share |
| --- | ---: | ---: |
| `planned` | 23 | 72% |
| `in_progress` | 4 | 12% |
| `ported` | 0 | 0% |
| `tested` | 5 | 16% |
| `production_wired` | 0 | 0% |
| `default` | 0 | 0% |
| `deferred` | 0 | 0% |
| **total** | **32** | 100% |

## hermes-1oa — Agent core runtime

Port AIAgent and the synchronous conversation/tool-call loop to Rust.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-1oa.1` | Define Rust agent-core domain model | `in_progress` | `run_agent.py`<br>`agent/model_metadata.py`<br>`agent/credential_pool.py` | `crates/hermes-agent-core` | `cargo test -p hermes-agent-core (workspace job)` |
| | _Initial scaffolding — Message/Role/AssistantTurn/ToolTurn, ToolCall, ToolResult, ToolDefinition, ToolFunction. Round-trips parity-fixture JSON. Pending against full acceptance criteria, expanded in subsequent ticks - budget/token state, compression metadata, provider routing inputs, interrupt and conversation outcome types._ |  |  |  |  |
| `hermes-1oa.2` | Port the AIAgent conversation loop | `planned` | `run_agent.py` | `crates/hermes-agent-core (planned)` | `tests/parity/agent_core/test_loop.py` |
| | _Synchronous tool-call iteration and budget accounting._ |  |  |  |  |
| `hermes-1oa.3` | Port compression and resume boundary behavior | `planned` | `agent/context_compressor.py`<br>`run_agent.py`<br>`hermes_state.py` | `crates/hermes-agent-core (planned)` | `tests/parity/agent_core/test_compression.py` |
| | _Head/tail preservation, summary fallback, lineage on split._ |  |  |  |  |
| `hermes-1oa.4` | Port provider-specific request and response handling | `planned` | `run_agent.py`<br>`agent/auxiliary_client.py`<br>`hermes_cli/runtime_provider.py` | `crates/hermes-agent-core (planned)` | `tests/parity/agent_core/test_providers.py` |
| | _OpenAI / Anthropic / Bedrock / Codex Responses message + tool encoding._ |  |  |  |  |

## hermes-4ne — Gateway runtime and platform orchestration

Port gateway session orchestration, slash commands, streaming delivery, adapter boundary.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-4ne.1` | Port gateway session orchestration and message guards | `planned` | `gateway/run.py`<br>`gateway/session.py` | `crates/hermes-gateway (planned)` | `tests/parity/gateway/test_session.py` |
| | _Active-session guards, queueing, interrupts, lifecycle._ |  |  |  |  |
| `hermes-4ne.2` | Port gateway slash and control command handling | `planned` | `gateway/run.py`<br>`hermes_cli/commands.py` | `crates/hermes-gateway (planned)` | `tests/parity/gateway/test_slash.py` |
| | _Slash + approvals must reach handler while agent is running._ |  |  |  |  |
| `hermes-4ne.3` | Port gateway streaming and delivery contracts | `planned` | `gateway/run.py`<br>`gateway/platforms/` | `crates/hermes-gateway (planned)` | `tests/parity/gateway/test_streaming.py` |
| | _Streaming, truncation, final metadata, media, retries._ |  |  |  |  |
| `hermes-4ne.4` | Define Rust platform adapter boundary and migrate adapters incrementally | `planned` | `gateway/platform_registry.py`<br>`gateway/platforms/` | `crates/hermes-gateway-adapter (planned)` | `tests/parity/gateway/test_adapter_trait.py` |
| | _Trait must let existing Python adapters keep working._ |  |  |  |  |

## hermes-k77 — Tool registry and tool execution

Port tool registry, toolset resolution, schemas, dispatch, approvals, and built-in tools.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-k77.1` | Port tool schema discovery and toolset resolution | `planned` | `tools/registry.py`<br>`model_tools.py`<br>`toolsets.py` | `crates/hermes-tools (planned)` | `tests/parity/tools/test_schemas.py` |
| | _Live registry should produce identical JSON Schemas in both backends._ |  |  |  |  |
| `hermes-k77.2` | Port function-call dispatch and error wrapping | `planned` | `model_tools.py` | `crates/hermes-tools (planned)` | `tests/parity/tools/test_dispatch.py` |
| | _handle_function_call result envelope and error normalization._ |  |  |  |  |
| `hermes-k77.3` | Port approval and safety guardrails for tools | `planned` | `tools/approval.py`<br>`agent/tool_guardrails.py` | `crates/hermes-tools-safety (planned)` | `tests/parity/tools/test_approvals.py` |
| | _Dangerous-cmd detect, yolo, file safety, gateway approvals._ |  |  |  |  |
| `hermes-k77.4` | Port core tool handlers by risk-ranked slices | `planned` | `tools/file_tools.py`<br>`tools/terminal_tool.py`<br>`tools/process_registry.py` | `crates/hermes-tools (planned)` | `tests/parity/tools/test_handlers.py` |
| | _Migrate by risk slice; not a single rewrite._ |  |  |  |  |

## hermes-ni1 — Parity gates and cutover governance

Golden parity gates, CI matrix, rollout controls, and cutover criteria.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-ni1.1` | Create subsystem parity matrix and status reporting | `in_progress` | `docs/rust-parity/status.yaml`<br>`scripts/rust_parity_status.py` | `n/a (governance artifact)` | `scripts/rust_parity_status.py --check` |
| | _This file is the matrix. Renderer also runs as a CI lint._ |  |  |  |  |
| `hermes-ni1.2` | Build golden transcript and tool-call fixtures | `in_progress` | `tests/parity/fixtures/`<br>`tests/parity/test_python_parity.py` | `tests/parity/test_rust_parity.py (skeleton)` | `tests/parity/` |
| | _Backend-agnostic JSON fixtures + Python loader. Rust loader follows hermes-1oa._ |  |  |  |  |
| `hermes-ni1.3` | Add CI matrix for Python, Rust, and mixed-backend modes | `tested` | `.github/workflows/tests.yml`<br>`scripts/run_tests.sh` | `.github/workflows/tests.yml (rust job)` | `GitHub Actions tests.yml — rust job` |
| | _Added a `rust` job that installs the stable Rust toolchain, runs `cargo test --workspace`, runs tests/rust/ + tests/parity/state/test_diagnostics.py (which need cargo), and runs the parity fixtures and matrix lint. Mandatory CI status (must-pass) is gated by hermes-te4.4._ |  |  |  |  |
| `hermes-ni1.4` | Document cutover, rollback, and default-backend criteria | `tested` | `docs/rust-parity/cutover.md` | `n/a (docs)` | `tests/parity/test_cutover_doc.py` |
| | _docs/rust-parity/cutover.md covers all 8 required sections (parity gates, perf gates, data compat, rollout flags, rollback steps, known deferrals, owner sign-off, post-cutover monitoring). Structural lint fails CI if a required section is missing or renamed. "tested" here means the doc structure is CI-enforced; status maps imperfectly to docs work._ |  |  |  |  |

## hermes-3n2 — CLI and configuration surfaces

Port or Rust-wrap CLI command dispatch, config/profile, setup flows, skins, logs.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-3n2.1` | Port CLI command registry and slash dispatch | `planned` | `cli.py`<br>`hermes_cli/main.py`<br>`hermes_cli/commands.py` | `crates/hermes-cli (planned)` | `tests/parity/cli/test_commands.py` |
| | _CommandDef-driven; aliases, categories, autocomplete._ |  |  |  |  |
| `hermes-3n2.2` | Port config, profile, and HERMES_HOME path semantics | `planned` | `hermes_cli/config.py`<br>`hermes_constants.py` | `crates/hermes-config (planned)` | `tests/parity/cli/test_config.py` |
| | _Profile isolation invariants must hold across both backends._ |  |  |  |  |
| `hermes-3n2.3` | Port setup, model, provider, and auth command flows | `planned` | `hermes_cli/main.py`<br>`hermes_cli/providers.py` | `crates/hermes-cli (planned)` | `tests/parity/cli/test_setup.py` |
| | _Prompt parity and config writes must be byte-identical for shared keys._ |  |  |  |  |
| `hermes-3n2.4` | Port CLI display, skins, logs, and status surfaces | `planned` | `cli.py`<br>`hermes_logging.py` | `crates/hermes-cli (planned)` | `tests/parity/cli/test_display.py` |
| | _Skin data, log routing, status command output shape._ |  |  |  |  |

## hermes-dwg — Integration surfaces

Port or integrate Rust for TUI gateway, ACP, dashboard backend, cron, batch, RL, MCP, plugins.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-dwg.1` | Port or bind TUI gateway backend protocol | `planned` | `tui_gateway/server.py` | `crates/hermes-tui-gateway (planned)` | `tests/parity/tui_gateway/test_protocol.py` |
| | _JSON-RPC contract used by Ink frontend must not change._ |  |  |  |  |
| `hermes-dwg.2` | Port ACP adapter session and tool integration | `planned` | `acp_adapter/server.py` | `crates/hermes-acp (planned)` | `tests/parity/acp/test_session.py` |
| | _Session, permissions, events, MCP tool registration._ |  |  |  |  |
| `hermes-dwg.3` | Port dashboard backend APIs without replacing embedded TUI | `planned` | `hermes_cli/web_server.py` | `crates/hermes-dashboard (planned)` | `tests/parity/dashboard/test_api.py` |
| | _Embedded TUI stays as primary chat surface._ |  |  |  |  |
| `hermes-dwg.4` | Port cron, batch, MCP, RL, and plugin-adjacent boundaries | `planned` | `cron/`<br>`batch_runner.py`<br>`mcp_serve.py`<br>`rl_cli.py` | `multiple (planned)` | `tests/parity/integrations/` |
| | _Migrate by surface; some may stay Python and call Rust core._ |  |  |  |  |

## hermes-izz — Production-grade state backend runtime

Replace the cargo-subprocess probe with a real production boundary.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-izz.1` | Replace subprocess probe with a production Rust boundary | `tested` | `hermes_state_rust.py` | `crates/hermes-state/src/bin/hermes_state_daemon.rs` | `cargo test -p hermes-state --test daemon + tests/parity/state/test_daemon_mode.py` |
| | _Standalone daemon binary listening on a Unix socket using length-prefixed JSON. Op handling is shared with the probe via crates/hermes-state/src/ops.rs so they cannot drift. RustSessionDB(boundary="daemon") autospawns the daemon, connects, and routes ops over the socket; HERMES_STATE_BOUNDARY env var honored. Subprocess boundary remains the default for back-compat. Idle daemon shuts down after 5 min by default._ |  |  |  |  |
| `hermes-izz.2` | Match SessionDB write contention and WAL behavior | `planned` | `hermes_state.py` | `crates/hermes-state` | `tests/parity/state/test_contention.py` |
| | _WAL, retry-with-jitter, busy_timeout — must match Python under contention._ |  |  |  |  |
| `hermes-izz.3` | State backend observability and rollback diagnostics | `in_progress` | `hermes_state_rust.py`<br>`hermes_state_factory.py` | `crates/hermes-state` | `tests/parity/state/test_diagnostics.py` |
| | _RustSessionDB.diagnostics() exposes backend, boundary, db_path, schema_version, op_count, error_count, last_error. Factory merges adapter snapshot into state_backend_diagnostics(db). Rollback diagnostics still pending._ |  |  |  |  |
| `hermes-izz.4` | Benchmark Rust state store against Python baseline | `planned` | `hermes_state.py` | `crates/hermes-state/benches/` | `scripts/bench_state.sh (planned)` |
| | _Objective perf data before defaulting to Rust._ |  |  |  |  |

## hermes-te4 — Production state-store cutover

Replace direct SessionDB construction with a backend factory and exercise the Rust path in real entry points.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-te4.1` | Replace direct SessionDB construction with a backend factory | `tested` | `hermes_state_factory.py`<br>`cli.py`<br>`gateway/run.py`<br>`gateway/session.py`<br>`gateway/mirror.py`<br>`gateway/platforms/api_server.py`<br>`hermes_cli/main.py`<br>`hermes_cli/web_server.py`<br>`hermes_cli/goals.py`<br>`tui_gateway/server.py`<br>`acp_adapter/session.py`<br>`mcp_serve.py`<br>`cron/scheduler.py`<br>`plugins/hermes-achievements/dashboard/plugin_api.py` | `hermes_state_factory.get_session_db` | `tests/parity/state/test_factory.py` |
| | _Factory landed; all production SessionDB() callsites migrated. Static-method imports (SessionDB.sanitize_title) intentionally left in place._ |  |  |  |  |
| `hermes-te4.2` | Add supported Rust state backend selection and diagnostics | `tested` | `hermes_state_factory.py` | `HERMES_STATE_BACKEND env + state.backend config key` | `tests/parity/state/test_factory.py` |
| | _Selection order arg → env → config → default(python). Explicit (arg/env) Rust failures raise; config-driven failures fall back to Python with a logged warning. state_backend_diagnostics() reports backend, source, fallback_reason, and (with db arg) merges adapter diagnostics._ |  |  |  |  |
| `hermes-te4.3` | Exercise Rust state backend in real production entry points | `planned` | `cli.py`<br>`gateway/run.py`<br>`hermes_cli/web_server.py` | `end-to-end smoke jobs` | `tests/parity/state/test_e2e_*.py` |
| | _Real CLI/gateway/dashboard runs against the Rust backend._ |  |  |  |  |
| `hermes-te4.4` | Make Rust state parity mandatory in CI | `planned` | `.github/workflows/tests.yml` | `required CI job` | `GitHub Actions required check` |
| | _Once green for two weeks, parity job becomes required._ |  |  |  |  |

## Existing Rust footprint (not yet on the cutover ladder)

| Path | State | Description |
| --- | --- | --- |
| `crates/hermes-state` | `tested-via-subprocess` | SQLite-backed state store with schema, search, title resolver, and a CLI probe. |

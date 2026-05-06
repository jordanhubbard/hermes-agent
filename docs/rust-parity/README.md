<!-- GENERATED FROM docs/rust-parity/status.yaml — edit the YAML and run scripts/rust_parity_status.py --write -->

# Rust parity matrix

Tracks the migration of Hermes subsystems from Python to Rust. Source of truth: `docs/rust-parity/status.yaml`.

## Constraints

**Rust crates do not link to in-repo Python.** Every Rust crate in this repo is a standalone reimplementation. It must not import or link in-repo Python code, must not be built as a Python extension whose sole purpose is to be loaded by CPython (e.g. PyO3 wheels), and must not embed CPython. Where a Python module in this repo wraps a third-party library, the Rust side reaches that library directly via a Rust crate (or its native interface) — never by calling back into the Python wrapper. This rule applies to every row in the matrix below.

**Contract parity is not full runtime parity.** Rows marked `tested` prove the Rust code matches Python for the scoped contract described by that row. They do not imply that the Rust binary can replace the Python runtime end to end. Full replacement requires every row in the `hermes-fpr` epic to reach `default` or to be explicitly deferred with owner sign-off.

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
| `planned` | 1 | 2% |
| `in_progress` | 5 | 12% |
| `ported` | 0 | 0% |
| `tested` | 36 | 86% |
| `production_wired` | 0 | 0% |
| `default` | 0 | 0% |
| `deferred` | 0 | 0% |
| **total** | **42** | 100% |

## hermes-1oa — Agent core runtime

Port AIAgent and the synchronous conversation/tool-call loop to Rust.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-1oa.1` | Define Rust agent-core domain model | `tested` | `run_agent.py`<br>`agent/model_metadata.py`<br>`agent/credential_pool.py`<br>`agent/context_compressor.py`<br>`hermes_cli/runtime_provider.py` | `crates/hermes-agent-core` | `cargo test -p hermes-agent-core (rust CI job)` |
| | _All acceptance categories present. Modules - message, tool, budget, compression, provider, outcome. Types - Role, Message, AssistantTurn, ToolTurn, ToolCall, ToolResult, ToolDefinition, ToolFunction, TokenUsage, TurnCost, ConversationBudget, CompressionEvent, CompressionTrigger, LineageTip, ApiMode, ProviderRouting, InterruptKind, ConversationOutcome, ConversationResult. 28 cargo tests cover serde round-trip and shape contracts (assistant null content preserved, default omits empty fields, externally-tagged outcome variants, saturating add on token usage, budget exhaustion semantics)._ |  |  |  |  |
| `hermes-1oa.2` | Port the AIAgent conversation loop | `tested` | `run_agent.py` | `crates/hermes-agent-core/src/conversation_loop.rs` | `cargo test -p hermes-agent-core + tests/parity/test_rust_parity.py` |
| | _Provider-independent synchronous loop landed with injected model/tool layers. Tests cover ordered assistant/tool persistence, tool dispatch ordering, max-iteration stop, one-turn grace call, interrupt-before-call, and final response shape. Real provider request/response handling remains tracked by hermes-1oa.4; real tool dispatch remains tracked by hermes-k77._ |  |  |  |  |
| `hermes-1oa.3` | Port compression and resume boundary behavior | `tested` | `agent/context_compressor.py`<br>`run_agent.py`<br>`hermes_state.py` | `crates/hermes-agent-core/src/compression_plan.rs + crates/hermes-state` | `cargo test -p hermes-agent-core + cargo test -p hermes-state compression/resume filters` |
| | _Rust compression planning preserves head/tail messages, inserts a structured summary with fallback text, counts dropped middle messages, and emits CompressionEvent lineage metadata. Rust state tests cover compression tips, ancestor replay without duplicate resume prompts, and title lineage across split sessions._ |  |  |  |  |
| `hermes-1oa.4` | Port provider-specific request and response handling | `tested` | `run_agent.py`<br>`agent/auxiliary_client.py`<br>`hermes_cli/runtime_provider.py` | `crates/hermes-agent-core/src/provider_wire.rs` | `cargo test -p hermes-agent-core --test provider_wire` |
| | _Provider wire helpers build Chat Completions, OpenAI-compatible, Responses, Anthropic, and Bedrock request payloads; parse Chat Completions, Responses/Codex-style, Anthropic, and Bedrock-compatible responses; normalize tool calls, reasoning fields, usage, finish reasons, streaming deltas, service tier, fallback model selection, and provider error classes. HTTP execution and credentials remain outside the core crate._ |  |  |  |  |

## hermes-4ne — Gateway runtime and platform orchestration

Port gateway session orchestration, slash commands, streaming delivery, adapter boundary.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-4ne.1` | Port gateway session orchestration and message guards | `tested` | `gateway/run.py`<br>`gateway/session.py` | `crates/hermes-gateway` | `cargo test -p hermes-gateway + tests/parity/gateway/test_session.py` |
| | _Rust gateway session guard now models active-session lifecycle, pending-message slots, FIFO overflow queueing, promotion semantics, empty-chain cleanup, busy-mode queueing, and command bypass decisions for stop/status-style control commands. Parity tests compare FIFO traces directly with Python GatewayRunner helpers and smoke the Rust lifecycle/bypass behavior._ |  |  |  |  |
| `hermes-4ne.2` | Port gateway slash and control command handling | `tested` | `gateway/run.py`<br>`hermes_cli/commands.py` | `crates/hermes-gateway` | `cargo test -p hermes-gateway + tests/parity/gateway/test_slash.py` |
| | _Rust gateway command router now uses the Rust CLI registry for canonical names and gateway-known filtering, matches Python's active-session bypass rule for every resolvable slash command, and classifies representative control flows including approve/deny, yolo, reload-mcp, reload-skills, title/resume, background/bg, queue, steer, status, help, and unsupported CLI-only commands._ |  |  |  |  |
| `hermes-4ne.3` | Port gateway streaming and delivery contracts | `tested` | `gateway/run.py`<br>`gateway/platforms/` | `crates/hermes-gateway` | `cargo test -p hermes-gateway + tests/parity/gateway/test_streaming.py` |
| | _Rust gateway streaming/delivery contracts now cover Python-compatible chunk truncation including code fences, inline-code split avoidance, UTF-16 length caps, MEDIA/audio marker cleanup, thread metadata, runtime footer rendering, retry/notice/fallback planning, and fresh-final delivery decisions including no-delete, short-lived, disabled, and send-failure paths._ |  |  |  |  |
| `hermes-4ne.4` | Define Rust platform adapter boundary and migrate adapters incrementally | `tested` | `gateway/platform_registry.py`<br>`gateway/platforms/` | `crates/hermes-gateway` | `cargo test -p hermes-gateway + tests/parity/gateway/test_adapter_trait.py` |
| | _Rust gateway crate now defines the platform adapter trait, normalized message/send/status/token-lock types, built-in platform value snapshot, and PlatformEntry metadata boundary. Parity tests verify the Rust boundary covers Python BasePlatformAdapter abstract methods and registry fields while an in-memory webhook adapter smoke exercises connect/start/send/receive/status/token-lock behavior end to end._ |  |  |  |  |

## hermes-k77 — Tool registry and tool execution

Port tool registry, toolset resolution, schemas, dispatch, approvals, and built-in tools.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-k77.1` | Port tool schema discovery and toolset resolution | `tested` | `tools/registry.py`<br>`model_tools.py`<br>`toolsets.py` | `crates/hermes-tools` | `cargo test -p hermes-tools + tests/parity/tools/test_schemas.py` |
| | _Rust tool registry crate resolves static, composite, legacy, all/*, and plugin-exposed toolsets; applies enabled/disabled filtering with disabled subtraction after enable; snapshots available schemas; and preserves cache-isolation behavior. Parity tests compare normalized Rust schemas and toolset outputs directly with Python model_tools/toolsets under hermetic credentials._ |  |  |  |  |
| `hermes-k77.2` | Port function-call dispatch and error wrapping | `tested` | `model_tools.py` | `crates/hermes-tools` | `cargo test -p hermes-tools + tests/parity/tools/test_dispatch.py` |
| | _Rust dispatch parity covers registry unknown/handler-exception error envelopes, handle_function_call argument coercion, agent-loop interception, pre-hook blocking and skip semantics, read-loop notifications, execute_code enabled-tool propagation, post-hook observation, transform-tool-result replacement, and outer exception normalization._ |  |  |  |  |
| `hermes-k77.3` | Port approval and safety guardrails for tools | `tested` | `tools/approval.py`<br>`agent/tool_guardrails.py` | `crates/hermes-tools::safety` | `cargo test -p hermes-tools + tests/parity/tools/test_approvals.py` |
| | _Rust safety parity covers dangerous and hardline command detection, Unicode/ANSI normalization, container bypasses, hardline-before-yolo/off ordering, process/session yolo, approval-mode off, cron deny/approve behavior, CLI once/session/always/deny outcomes, gateway approval-required/approve/deny/timeout outcomes, smart approve/deny, permanent/session allowlists, and the side-effect-free tool loop guardrail controller._ |  |  |  |  |
| `hermes-k77.4` | Port core tool handlers by risk-ranked slices | `tested` | `tools/file_tools.py`<br>`tools/terminal_tool.py`<br>`tools/process_registry.py` | `crates/hermes-tools::handlers + documented Python boundaries` | `cargo test -p hermes-tools + tests/parity/tools/test_handlers.py` |
| | _Rust native handler slice covers read_file line-window output, search_files content/file modes, write_file directory creation and byte counts, patch replace-mode result envelopes, post-patch content, protected write denial, todo replace/merge/read/injection semantics, clarify validation/result shaping, memory add/replace/remove/threat-scan/snapshot semantics, session_search dispatcher/recent/lineage/raw-preview behavior, local skills_list/skill_view plus skill_manage create/edit/patch/delete/supporting-file mutation semantics, and Home Assistant validation/filtering/payload/result-envelope semantics. Terminal/process, browser/web, delegate/subagent, MCP, session_search production wiring to hermes-state plus auxiliary summarization, media, plugin/optional-skill/setup/provenance/slash-injection flows, clarify UI callbacks, cron/messaging, Home Assistant production HTTP wiring, and kanban handlers remain documented deletion-blocking runtime boundaries in docs/rust-parity/tool-handler-boundaries.md with parity tests asserting every core tool is either native or explicitly covered before cutover._ |  |  |  |  |

## hermes-ni1 — Parity gates and cutover governance

Golden parity gates, CI matrix, rollout controls, and cutover criteria.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-ni1.1` | Create subsystem parity matrix and status reporting | `tested` | `docs/rust-parity/status.yaml`<br>`scripts/rust_parity_status.py` | `n/a (governance artifact)` | `scripts/rust_parity_status.py --check` |
| | _status.yaml is the source of truth, README.md is generated from it, and scripts/rust_parity_status.py --check is enforced by tests/parity/test_parity_matrix.py._ |  |  |  |  |
| `hermes-ni1.2` | Build golden transcript and tool-call fixtures | `tested` | `tests/parity/fixtures/`<br>`tests/parity/test_python_parity.py` | `crates/hermes-agent-core/src/replay.rs + crates/hermes-agent-core/src/bin/hermes_agent_replay.rs` | `tests/parity/` |
| | _Five backend-agnostic fixtures cover plain chat, single tool call, multi-turn tool use, reasoning fields, and tool-error recovery. Python reference replay and Rust hermes_agent_replay both emit ReplayResult-shaped payloads checked by the same expected blocks._ |  |  |  |  |
| `hermes-ni1.3` | Add CI matrix for Python, Rust, and mixed-backend modes | `tested` | `.github/workflows/tests.yml`<br>`scripts/run_tests.sh` | `.github/workflows/tests.yml (rust job)` | `GitHub Actions tests.yml — rust job` |
| | _Added a `rust` job that installs the stable Rust toolchain, runs `cargo test --workspace`, runs tests/rust/ + tests/parity/state/test_diagnostics.py (which need cargo), and runs the parity fixtures and matrix lint. Mandatory CI status (must-pass) is gated by hermes-te4.4._ |  |  |  |  |
| `hermes-ni1.4` | Document cutover, rollback, and default-backend criteria | `tested` | `docs/rust-parity/cutover.md` | `n/a (docs)` | `tests/parity/test_cutover_doc.py` |
| | _docs/rust-parity/cutover.md covers all 8 required sections (parity gates, perf gates, data compat, rollout flags, rollback steps, known deferrals, owner sign-off, post-cutover monitoring). Structural lint fails CI if a required section is missing or renamed. "tested" here means the doc structure is CI-enforced; status maps imperfectly to docs work._ |  |  |  |  |

## hermes-3n2 — CLI and configuration surfaces

Port or Rust-wrap CLI command dispatch, config/profile, setup flows, skins, logs.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-3n2.1` | Port CLI command registry and slash dispatch | `tested` | `cli.py`<br>`hermes_cli/main.py`<br>`hermes_cli/commands.py` | `crates/hermes-cli` | `cargo test -p hermes-cli + tests/parity/cli/test_commands.py` |
| | _Rust command registry now owns CommandDef metadata, alias resolution, CLI help/category maps, subcommands, gateway-known command sets, config-gated gateway help/Telegram/Slack surfaces, and representative slash-dispatch parsing. Parity tests compare Rust snapshots directly to hermes_cli.commands under default and config-gated configs._ |  |  |  |  |
| `hermes-3n2.2` | Port config, profile, and HERMES_HOME path semantics | `tested` | `hermes_cli/config.py`<br>`hermes_constants.py` | `crates/hermes-config` | `cargo test -p hermes-config + tests/parity/cli/test_config.py` |
| | _Rust config/profile layer now covers HERMES_HOME/default-root/profile-root/display path semantics, recursive config deep merge, env-template expansion, legacy max_turns migration, root model-key normalization, and selected CLI-vs-gateway env bridge behavior. Parity tests compare Rust probes directly with Python helpers for default, native profile, custom root, and custom profile layouts._ |  |  |  |  |
| `hermes-3n2.3` | Port setup, model, provider, and auth command flows | `tested` | `hermes_cli/main.py`<br>`hermes_cli/providers.py` | `crates/hermes-cli` | `cargo test -p hermes-cli + tests/parity/cli/test_setup.py` |
| | _Rust setup/auth planning covers provider metadata, aliases, API-mode resolution, model config write shape, same-provider credential-pool eligibility, auth command surfaces, and secret-storage boundaries. Parity tests compare Rust snapshots with Python hermes_cli.providers/setup helpers and verify API keys remain out of config.yaml._ |  |  |  |  |
| `hermes-3n2.4` | Port CLI display, skins, logs, and status surfaces | `tested` | `cli.py`<br>`hermes_logging.py` | `crates/hermes-cli` | `cargo test -p hermes-cli + tests/parity/cli/test_display.py` |
| | _Rust CLI display layer now snapshots stable built-in skin surface values, response/status metadata, log file planning for CLI/gateway modes, and status output rendering. Parity tests compare the Rust snapshot against Python skin_engine/HermesCLI output and assert display sources do not reintroduce ANSI erase-to-EOL._ |  |  |  |  |

## hermes-dwg — Integration surfaces

Port or integrate Rust for TUI gateway, ACP, dashboard backend, cron, batch, RL, MCP, plugins.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-dwg.1` | Port or bind TUI gateway backend protocol | `tested` | `tui_gateway/server.py` | `crates/hermes-tui-gateway` | `tests/parity/tui_gateway/test_protocol.py` |
| | _Rust TUI gateway crate now owns the JSON-RPC method/event protocol snapshot, long-handler routing contract, prompt/tool/approval stream event sequences, and JSON-RPC error envelope behavior. Parity tests compare the Rust catalog against Python @method registrations, _LONG_HANDLERS, static Python emitters, and ui-tui production request/event type expectations._ |  |  |  |  |
| `hermes-dwg.2` | Port ACP adapter session and tool integration | `tested` | `acp_adapter/server.py` | `crates/hermes-acp` | `tests/parity/acp/test_session.py` |
| | _Rust ACP crate now owns a typed contract snapshot for ACP capabilities, public server methods, slash-command advertisements, session state/persistence metadata, permission outcome mapping, event callback routing, tool kind/title/rendering contracts, and the Python runtime boundary. Parity tests compare the Rust snapshot directly against acp_adapter server/session/permissions/tools behavior, and the existing ACP test suite passes with the declared ACP/dev test extras installed._ |  |  |  |  |
| `hermes-dwg.3` | Port dashboard backend APIs without replacing embedded TUI | `tested` | `hermes_cli/web_server.py` | `crates/hermes-dashboard` | `tests/parity/dashboard/test_api.py` |
| | _Rust dashboard crate now owns a typed contract snapshot for the built-in FastAPI route table, REST auth/public-path split, WebSocket close-code/channel semantics, React API-client coverage, and the embedded chat boundary. Parity tests compare the Rust snapshot against hermes_cli/web_server.py decorators and middleware constants plus web/src production client usage, and assert the chat tab still renders the real hermes --tui through xterm.js over /api/pty rather than a duplicate React transcript/composer._ |  |  |  |  |
| `hermes-dwg.4` | Port cron, batch, MCP, RL, and plugin-adjacent boundaries | `tested` | `cron/`<br>`batch_runner.py`<br>`mcp_serve.py`<br>`rl_cli.py` | `crates/hermes-integrations + docs/rust-parity/integration-boundaries.md` | `tests/parity/integrations/` |
| | _Rust integrations crate now owns contract snapshots for cron job/scheduler/delivery boundaries, batch runner CLI/output schemas, MCP FastMCP tool/event surface, RL CLI/AIAgent invocation settings, and dynamic plugin registration/dashboard APIs. docs/rust-parity/integration-boundaries.md records the explicit Python runtime boundaries. Parity tests compare the Rust snapshot against Python function signatures, constants, MCP decorators, plugin facade methods, hooks, manifest fields, and dashboard helpers._ |  |  |  |  |

## hermes-izz — Production-grade state backend runtime

Replace the cargo-subprocess probe with a real production boundary.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-izz.1` | Replace subprocess probe with a production Rust boundary | `tested` | `hermes_state_rust.py` | `crates/hermes-state/src/bin/hermes_state_daemon.rs` | `cargo test -p hermes-state --test daemon + tests/parity/state/test_daemon_mode.py` |
| | _Standalone daemon binary listening on a Unix socket using length-prefixed JSON. Op handling is shared with the probe via crates/hermes-state/src/ops.rs so they cannot drift. RustSessionDB(boundary="daemon") autospawns the daemon, connects, and routes ops over the socket; HERMES_STATE_BOUNDARY env var honored. Subprocess boundary remains the default for back-compat. Idle daemon shuts down after 5 min by default._ |  |  |  |  |
| `hermes-izz.2` | Match SessionDB write contention and WAL behavior | `tested` | `hermes_state.py` | `crates/hermes-state/src/bin/hermes_state_daemon.rs (Mutex<SessionStore>)` | `tests/parity/state/test_daemon_concurrency.py + crates/hermes-state/tests/daemon.rs` |
| | _Daemon handles connections concurrently (thread-per-connection with a shared Mutex<SessionStore>) so a long-lived client never blocks others. SQLite remains single-writer through the mutex, which sidesteps the multi-process WAL retry-with-jitter loop the Python SessionDB needs. 8 concurrent writers x 10 ops each round-trip without data loss; per-thread FIFO ordering preserved; bad ops from one client do not taint others._ |  |  |  |  |
| `hermes-izz.3` | State backend observability and rollback diagnostics | `tested` | `hermes_state_rust.py`<br>`hermes_state_factory.py` | `crates/hermes-state` | `tests/parity/state/test_diagnostics.py` |
| | _RustSessionDB.diagnostics() exposes backend, boundary, db_path, schema_version, migration_action, op_count, error_count, last_error, and last_error_class. Initialization logs db path, schema version, and migration action. rollback_diagnostics() opens the same DB through Python SessionDB and reports python_readable, schema_version, session_count, and any error class. Factory logs selected backend with db_path and merges adapter snapshots via state_backend_diagnostics(db)._ |  |  |  |  |
| `hermes-izz.4` | Benchmark Rust state store against Python baseline | `tested` | `hermes_state.py`<br>`scripts/bench_state.py` | `scripts/bench_state.py (drives Python + Rust subprocess + Rust daemon)` | `tests/parity/state/test_bench_harness.py` |
| | _Headline numbers (M2 Pro, 30 create+append pairs per backend) — Python 0.29 ms/op (3471 ops/s) baseline, subprocess 270.30 ms/op (3.7 ops/s, 941x slower than Python — unviable), daemon 0.36 ms/op (2745 ops/s, 1.26x slower than Python — acceptable for the IPC cost). Validates the cutover argument that the daemon brings Rust within reasonable distance of Python while subprocess is a non-starter._ |  |  |  |  |

## hermes-te4 — Production state-store cutover

Replace direct SessionDB construction with a backend factory and exercise the Rust path in real entry points.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-te4.1` | Replace direct SessionDB construction with a backend factory | `tested` | `hermes_state_factory.py`<br>`cli.py`<br>`gateway/run.py`<br>`gateway/session.py`<br>`gateway/mirror.py`<br>`gateway/platforms/api_server.py`<br>`hermes_cli/main.py`<br>`hermes_cli/web_server.py`<br>`hermes_cli/goals.py`<br>`tui_gateway/server.py`<br>`acp_adapter/session.py`<br>`mcp_serve.py`<br>`cron/scheduler.py`<br>`plugins/hermes-achievements/dashboard/plugin_api.py` | `hermes_state_factory.get_session_db` | `tests/parity/state/test_factory.py` |
| | _Factory landed; all production SessionDB() callsites migrated. Static-method imports (SessionDB.sanitize_title) intentionally left in place._ |  |  |  |  |
| `hermes-te4.2` | Add supported Rust state backend selection and diagnostics | `tested` | `hermes_state_factory.py` | `HERMES_STATE_BACKEND env + state.backend config key` | `tests/parity/state/test_factory.py` |
| | _Selection order arg → env → config → default(python). Explicit (arg/env) Rust failures raise; config-driven failures fall back to Python with a logged warning. state_backend_diagnostics() reports backend, source, fallback_reason, and (with db arg) merges adapter diagnostics._ |  |  |  |  |
| `hermes-te4.3` | Exercise Rust state backend in real production entry points | `tested` | `cli.py`<br>`gateway/run.py`<br>`hermes_cli/web_server.py` | `tests/parity/state/test_e2e_factory_daemon.py` | `tests/parity/state/test_e2e_factory_daemon.py` |
| | _e2e test drives env -> factory -> RustSessionDB(daemon) -> daemon binary -> SQLite through a realistic session lifecycle (create, append user/assistant/tool/reasoning messages, update tokens, FTS search, rich list, end, delete) — the same call shapes cli.py, gateway/run.py, hermes_cli/web_server.py use. Subprocess-invocation smoke for the literal hermes CLI binary is a follow-up; the underlying behavior is already gated._ |  |  |  |  |
| `hermes-te4.4` | Make Rust state parity mandatory in CI | `tested` | `.github/workflows/tests.yml` | `required CI job` | `GitHub Actions tests.yml — rust job` |
| | _The rust job now runs cargo test --workspace, tests/rust/, targeted Rust-state diagnostics and literal hermes CLI smoke tests, then the full tests/parity/ suite. Because it is part of the Tests workflow, any rust-job failure marks the workflow failed; branch-protection UI can additionally require the job name, but the code-level CI gate is in place._ |  |  |  |  |

## hermes-fpr — Full parity and Python removal

Convert scoped Rust parity into a Rust-primary production runtime and remove Python only after every supported workflow is covered.

| Bead | Story | Status | Python | Rust target | CI gate |
| --- | --- | --- | --- | --- | --- |
| `hermes-fpr.1` | Audit every Python entry point against Rust-primary ownership | `tested` | `run_agent.py`<br>`cli.py`<br>`hermes_cli/main.py`<br>`gateway/run.py`<br>`tui_gateway/server.py`<br>`hermes_cli/web_server.py`<br>`acp_adapter/`<br>`cron/`<br>`batch_runner.py`<br>`mcp_serve.py`<br>`rl_cli.py`<br>`tools/`<br>`plugins/` | `docs/rust-parity/entrypoint-audit.md + docs/rust-parity/full-parity-plan.md + docs/rust-parity/status.yaml` | `tests/parity/test_full_parity_plan.py` |
| | _Entry-point audit records that no installed user-facing Hermes command is Rust-primary yet. It maps CLI, agent, gateway, TUI, dashboard, ACP, cron, batch, MCP, tools, skills, plugins, state, and packaging surfaces to current Rust ownership, blockers, deletion risks, and required smoke tests._ |  |  |  |  |
| `hermes-fpr.2` | Ship a Rust-owned hermes binary and runtime selector | `tested` | `hermes_cli/main.py`<br>`pyproject.toml`<br>`scripts/install.sh` | `crates/hermes-cli/src/bin/hermes.rs + scripts/install.sh + hermes_cli/main.py update relink` | `tests/parity/cli/test_rust_launcher.py` |
| | _Rust-owned launcher selects runtime via HERMES_RUNTIME, reports runtime info, runs Rust-native launcher commands, rejects unported Rust commands without importing Python, and executes explicit Python fallback through python -m hermes_cli.main. install.sh builds and links target/release/hermes when cargo is available; hermes update rebuilds and relinks existing Hermes-owned symlinks. Production workflow parity remains tracked by later fpr rows._ |  |  |  |  |
| `hermes-fpr.3` | Make the Rust agent loop production-capable | `tested` | `run_agent.py`<br>`agent/`<br>`model_tools.py` | `crates/hermes-agent-core/src/provider_http.rs + crates/hermes-agent-core/src/runtime.rs + crates/hermes-agent-core/src/runtime_state.rs` | `cargo test -p hermes-agent-core --test provider_http --test runtime --test credentials --test runtime_state + tests/parity/cli/test_rust_launcher.py` |
| | _Provider HTTP execution reuses provider_wire request/response parity and has mock OpenAI-compatible E2E tests for request URL, auth/header forwarding, body shape, parsed assistant response, usage, 429 retry classification, and streaming SSE delta collection. Credential resolution covers explicit keys, provider env ordering, aliases, same-provider pools, disabled entries, and secret redaction. Runtime.rs adds a production-shaped synchronous loop over model/tool/store/hook traits with fallback, interrupt-before-call, budget usage accumulation, compression planning, tool dispatch, persistence callbacks, token accounting, and lifecycle hooks. StateConversationStore persists runtime messages, tool calls, reasoning, token totals, and compression lineage through hermes-state. The Rust launcher now has HERMES_RUNTIME=rust hermes agent-runtime-smoke to prove the installed Rust binary can execute the runtime slice without importing Python. This does not make Hermes fully Rust-default; tool handlers, gateway, CLI, integrations, plugins/skills, shadow execution, and Python deletion remain tracked by later fpr rows._ |  |  |  |  |
| `hermes-fpr.4` | Port all tool handlers or gate explicit non-removable boundaries | `in_progress` | `tools/`<br>`model_tools.py`<br>`toolsets.py` | `crates/hermes-tools` | `all core tool E2E tests pass with Rust dispatch and Rust handlers` |
| | _Started the full handler cutover gate by adding native Rust todo semantics, clarify validation/result shaping, memory add/replace/remove/threat-scan/snapshot semantics, session_search dispatcher/recent/lineage/raw-preview semantics, local skills_list/skill_view plus skill_manage create/edit/patch/delete/supporting-file mutation semantics, cronjob API validation/local CRUD result shaping, Home Assistant validation/filtering/payload/result-envelope semantics, and an executable coverage check over all 45 core tools from the Rust registry snapshot. Current state remains not deletion-safe; terminal/process/execute_code, browser/web, delegate/subagent, MCP dynamic discovery, session_search production wiring to hermes-state plus auxiliary summarization, media, plugin/optional-skill/setup/provenance/slash-injection flows, clarify UI callbacks, cron scheduler persistence/tick execution/gateway delivery plus send_message clients, Home Assistant production HTTP wiring, and kanban are explicit deletion blockers with required Rust or external-service migration plans in docs/rust-parity/tool-handler-boundaries.md._ |  |  |  |  |
| `hermes-fpr.5` | Port gateway runner and production platform adapters | `in_progress` | `gateway/run.py`<br>`gateway/platforms/`<br>`gateway/session.py` | `crates/hermes-gateway` | `gateway smoke matrix for every built-in platform adapter with Rust runtime` |
| | _Started production gateway CLI cutover with Rust-owned `hermes gateway status` for the no-service/manual status path and `hermes gateway stop` for the no-running profile path, including not-running output and recent runtime health lines from gateway_state.json matched against Python. Actual gateway termination still falls back when pid/lock files exist. Remaining gateway lifecycle, run/start/running-stop/restart/install/uninstall/setup flows, systemd/launchd/Termux/WSL branches, process scanning/token locks, session guards, approvals, background notifications, slash commands, delivery, restart/update behavior, and platform adapter smokes remain Python-owned._ |  |  |  |  |
| `hermes-fpr.6` | Port CLI setup/auth/model/config/update/profile/log/skin surfaces | `in_progress` | `cli.py`<br>`hermes_cli/`<br>`hermes_constants.py`<br>`hermes_logging.py` | `crates/hermes-cli + crates/hermes-config` | `CLI smoke suite from clean temp HOME and migrated profile fixtures` |
| | _Started the production CLI cutover with Rust launcher-side -p/--profile resolution for Rust-owned commands plus native `hermes auth list [provider]`, `hermes auth remove <provider> <target>`, `hermes auth reset <provider>`, `hermes auth status <provider>`, `hermes profile`, `hermes profile list`, `hermes profile show`, `hermes profile use`, `hermes profile delete -y`, `hermes profile rename`, `hermes profile alias`, `hermes config`/`config show`, `hermes config path`, `hermes config env-path`, `hermes config set`, and bounded `hermes logs` list/tail behavior matching Python for clean default/named profile homes, display paths, credential-pool listing with provider filters, id/label/index credential removal, removal suppression/cleanup for known persisted credential sources, exhausted auth-failure status text, credential status resets, API-key/OAuth/Spotify auth status display, config summary display, config model/provider rendering, skill counts, .env/SOUL status, table rendering, sticky active-profile writes, non-interactive profile deletion with active-profile reset, profile rename directory moves, wrapper alias updates, active-profile updates, custom profile alias creation/removal, config/.env path reporting, dotted config writes, list-index config writes, API-key .env writes, terminal env sync, log listing, non-follow tailing, level/session/component log filters, and relative `--since` log filtering when no gateway is running. Remaining setup wizard, auth add/logout/Spotify login/logout, provider/model selection, config edit/check/migrate, dynamic skill-settings display in config show, profile create/interactive-delete/export/import, live gateway-running detection in profile output, logs follow mode, update flow, skins/display mutation, remaining skills commands, and interactive/TUI behavior remain Python-owned._ |  |  |  |  |
| `hermes-fpr.7` | Port integration runtimes around the primary agent | `in_progress` | `tui_gateway/`<br>`hermes_cli/web_server.py`<br>`acp_adapter/`<br>`cron/`<br>`batch_runner.py`<br>`mcp_serve.py`<br>`rl_cli.py` | `crates/hermes-tui-gateway + crates/hermes-dashboard + crates/hermes-acp + crates/hermes-integrations` | `TUI/dashboard/ACP/cron/batch/MCP/RL E2E tests with Rust runtime` |
| | _Started production integration cutover with Rust-owned `hermes cron status`, `hermes cron list [--all]`, `hermes cron pause`, `hermes cron resume`, and `hermes cron remove/rm/delete`, matching Python for no-gateway status, active-job counting, disabled-job exclusion, next-run display, empty-list messaging, job table rendering, disabled-job inclusion, skill normalization, repeat/delivery/script/workdir/last-run fields, gateway-not-running warnings, pause state mutation, resume state mutation and next-run recomputation for interval/eligible once schedules, remove mutation, missing-job messages, and Python's current zero exit status for cron lifecycle failures from cron/jobs.json. Remaining TUI gateway, dashboard backend, ACP, cron create/edit/run/tick scheduler execution, cron-expression next-run parity in CLI resume, batch, MCP, RL, real session/state/tool/provider integration, and delivery runtimes remain Python-owned._ |  |  |  |  |
| `hermes-fpr.8` | Preserve skills and define Rust plugin migration policy without in-repo Python | `in_progress` | `skills/`<br>`optional-skills/`<br>`plugins/`<br>`hermes_cli/plugins.py` | `crates/hermes-cli + Rust skill loader and plugin migration/deferral policy` | `Rust skill smoke tests plus plugin list/enable/disable CLI parity and repo-plugin migration/deferral review` |
| | _Started with Rust-owned `hermes plugins list`, `hermes plugins enable`, `hermes plugins disable`, `hermes plugins remove/rm/uninstall`, and `hermes skills list`, matching Python on bundled/user plugin manifest listing, enabled/disabled plugin config writes, user plugin directory removal, missing-plugin remove errors, empty skills hub initialization, installed skill source classification, source filtering, enabled-only filtering, global disabled skill status rendering, and per-platform disabled skill filtering via `HERMES_PLATFORM`/`HERMES_SESSION_PLATFORM`. Per owner direction, Rust and Python plugin ABIs do not need to be mutually compatible unless an explicit RPC/IPC contract already exists; external user/pip Python plugins are not deletion blockers and can be converted to Rust on demand. Skills install/update/audit/snapshot/slash injection, optional skill registries, in-repo Python plugins, plugin install/update flows, plugin platform adapters, memory/image/context providers, dashboard plugin APIs, and any existing RPC-backed extension contracts still need Rust ports or signed deferrals before Python source removal._ |  |  |  |  |
| `hermes-fpr.9` | Run shadow Python-vs-Rust execution and divergence triage | `tested` | `tests/parity/`<br>`scripts/` | `scripts/rust_shadow_diff.py` | `tests/parity/test_shadow_diff.py` |
| | _scripts/rust_shadow_diff.py dual-runs representative Python and Rust surfaces and fails on unexplained divergence. Current covered cases compare all golden agent replay fixtures for prompts/tool calls, CLI command registry and dispatch samples, gateway control-command routing, and a mutable session lifecycle through Python SessionDB vs the Rust state daemon. The harness reports divergence classifications and tests/parity/test_shadow_diff.py blocks CI on any unexplained difference. This does not make Rust default or deletion-safe; it is a shadow gate over the Rust-owned surfaces that exist so far._ |  |  |  |  |
| `hermes-fpr.10` | Flip Rust to default and remove Python sources | `planned` | `run_agent.py`<br>`cli.py`<br>`model_tools.py`<br>`hermes_cli/`<br>`gateway/`<br>`tui_gateway/`<br>`acp_adapter/`<br>`tools/`<br>`plugins/` | `Rust default runtime and deletion commit` | `scripts/run_tests.sh equivalent with Rust default + cargo test --workspace + clean install smoke` |
| | _This is a final gate, not an implementation bead. Python removal is allowed only after every previous fpr row is default or explicitly deferred with owner sign-off and rollback/release notes are complete._ |  |  |  |  |

## Existing Rust footprint (not yet on the cutover ladder)

| Path | State | Description |
| --- | --- | --- |
| `crates/hermes-state` | `tested-via-subprocess` | SQLite-backed state store with schema, search, title resolver, and a CLI probe. |

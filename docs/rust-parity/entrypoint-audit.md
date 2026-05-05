# Rust Full-Parity Entry-Point Audit

This audit is the completion artifact for `hermes-fpr.1`. It maps the Python
runtime entry points that must be owned by Rust, explicitly gated as external
boundaries, or left as deletion blockers before the in-repo Python sources can
be removed.

## Classification

- `rust-primary`: Rust owns the production runtime path and does not import
  in-repo Python.
- `python-primary`: The shipped runtime still enters Python. Rust may already
  own tested contracts for the surface, but not the live workflow.
- `contract-tested`: Rust has parity coverage for stable schemas, protocol
  shapes, or helper behavior. This is not a runtime replacement.
- `external-boundary`: The surface may stay outside Rust only if it remains
  usable after in-repo Python source deletion through an explicit IPC, process,
  ABI, or user-extension boundary.
- `deletion-blocker`: Deleting in-repo Python today would break the workflow.

## Audit Summary

As of `hermes-fpr.2`, installs and updates can expose a Rust-owned `hermes`
launcher when Cargo is available. That launcher owns runtime selection through
`HERMES_RUNTIME` and provides an explicit Python rollout fallback. This is not
full runtime parity: the launcher still routes production chat, gateway, TUI,
dashboard, ACP, tools, skills, and plugin workflows to Python unless a Rust-owned
command is selected.

`pyproject.toml` still declares Python console scripts for pip/editable-package
fallback compatibility:

- `hermes = "hermes_cli.main:main"`
- `hermes-agent = "run_agent:main"`
- `hermes-acp = "acp_adapter.entry:main"`

The Rust workspace contains tested crates and support binaries, including the
state daemon, replay runner, and snapshot tools. Those prove scoped contracts.
They do not yet own top-level command dispatch, live model HTTP execution, live
gateway/platform serving, the dashboard server, the TUI gateway runtime, ACP
stdio serving, dynamic tools, skills, or plugins.

The main partial exception is state storage: `crates/hermes-state` has a tested
daemon backend and can be selected through the Python `hermes_state_factory`.
That still leaves the application entry points Python-primary because the Rust
backend is reached through Python adapters.

## Entry Point Inventory

| Surface | Python runtime entry | Current Rust ownership | Status | Blocking work | Required smoke/parity gate |
| --- | --- | --- | --- | --- | --- |
| Installed `hermes` command and top-level dispatch | `pyproject.toml`, `hermes_cli/main.py`, `scripts/install.sh` | `crates/hermes-cli/src/bin/hermes.rs` owns the install/update launcher and `HERMES_RUNTIME` selector. `HERMES_RUNTIME=python` explicitly executes `python -m hermes_cli.main`; `HERMES_RUNTIME=rust` runs only Rust-owned launcher commands today. Python argparse dispatch remains the fallback for production workflows. | `runtime-selector-tested`, `python-primary workflow fallback`, `deletion-blocker` | `hermes-fpr.6`, `hermes-fpr.10` | Clean install/update smoke for launcher build/linking, `hermes --runtime-info`, `HERMES_RUNTIME=rust hermes version`, explicit Python fallback, and eventual Rust-owned `hermes -q`, setup, config, profile, logs, skills, and tools commands. |
| Classic interactive and single-query chat CLI | `cli.py`, `HermesCLI`, `AIAgent` | Rust has registry/config/display contracts, but the interactive loop, prompt handling, credential checks, image preprocessing, worktree handling, signal handling, and `AIAgent` invocation remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.3`, `hermes-fpr.6` | Rust default smoke for interactive startup, quiet `-q`, image input, resume, skills preload, worktree mode, credential failure, and interrupted tool execution. |
| Direct `hermes-agent` command | `pyproject.toml`, `run_agent.py` | `crates/hermes-agent-core` owns domain types, canned conversation loop, provider wire helpers, compression planning, and fixture replay. It does not execute live provider HTTP or production tool dispatch. | `python-primary`, `deletion-blocker` | `hermes-fpr.3`, `hermes-fpr.4` | Mock-provider E2E covering streaming and non-streaming responses, tool calls, failures, budgets, interrupts, compression, persistence, and lifecycle hooks through the Rust runtime. |
| Gateway command and service lifecycle | `hermes_cli/main.py`, `cli.py --gateway`, `gateway/run.py` | `crates/hermes-gateway` owns guard, slash-route, streaming/delivery, and adapter-trait contracts. It does not own `GatewayRunner`, platform startup, live delivery, approvals, restart/update, or token-lock lifecycle. | `python-primary`, `deletion-blocker` | `hermes-fpr.5` | Built-in platform smoke matrix for telegram, discord, slack, whatsapp, signal, matrix, mattermost, email, sms, homeassistant, dingtalk, wecom, weixin, feishu, qqbot, bluebubbles, webhook, api_server, and plugin platforms with Rust runtime. |
| Gateway platform adapters | `gateway/platforms/`, `gateway/platform_registry.py` | Rust has a normalized adapter trait and built-in platform value snapshot. Production adapters are Python classes. | `python-primary`, `external-boundary candidate`, `deletion-blocker` | `hermes-fpr.5`, `hermes-fpr.8` | Connect/start/send/receive/status/token-lock smoke for each built-in adapter or signed external-boundary decision per adapter. |
| TUI backend for `hermes --tui` | `hermes_cli/main.py`, `ui-tui/`, `tui_gateway/entry.py`, `tui_gateway/server.py` | `crates/hermes-tui-gateway` owns JSON-RPC protocol snapshots. The Rust snapshot labels live methods `python_bound`; live session, prompt, slash, approval, completion, and tool events remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.3`, `hermes-fpr.6` | Ink TUI E2E against Rust gateway backend for session create/resume, prompt submit, slash commands, completions, approvals, tool progress, interrupts, and crash/EOF handling. |
| Dashboard backend and embedded chat | `hermes_cli/main.py dashboard`, `hermes_cli/web_server.py`, `hermes_cli/pty_bridge.py` | `crates/hermes-dashboard` owns route/auth/WebSocket/embedded-chat contracts. FastAPI handlers, config/env/profile/session/model/cron/plugin APIs, and PTY spawning remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.6`, `hermes-fpr.8` | Rust dashboard smoke for REST auth, config/env mutation, sessions, model selection, cron APIs, profile APIs, plugin APIs, `/api/pty`, and dashboard WebSockets without replacing the embedded TUI. |
| ACP command and stdio server | `pyproject.toml`, `hermes_cli/main.py acp`, `acp_adapter/entry.py`, `acp_adapter/server.py` | `crates/hermes-acp` owns capability, method, session, permission, tool-rendering, and event-callback contracts. Live ACP stdio serving and `AIAgent` execution remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.3`, `hermes-fpr.4` | ACP client smoke for initialize, new/resume/fork sessions, prompt with image, tool approvals, cancellation, permissions, and persisted state under Rust runtime. |
| Cron CLI, scheduler, and cron tool | `hermes_cli/main.py cron`, `cron/jobs.py`, `cron/scheduler.py`, `tools/cronjob_tools.py` | `crates/hermes-integrations` owns schedule/job/delivery contract snapshots. Live tick locking, job execution, gateway delivery, file output, and agent invocation remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.4`, `hermes-fpr.5` | Rust cron smoke for create/list/edit/pause/resume/run/remove/tick, persisted jobs, local output, gateway delivery, and cron tool calls. |
| Batch runner | `batch_runner.py` | `crates/hermes-integrations` owns CLI argument, result schema, and output/checkpoint contract snapshots. Multiprocessing, dataset IO, trajectory assembly, and `AIAgent` calls remain Python. | `python-primary`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.3`, `hermes-fpr.4` | Rust batch smoke over a small JSONL dataset with resume/checkpoint, deterministic mock provider, output files, failed samples, and trajectory schema comparison. |
| MCP server and dynamic MCP tools | `hermes_cli/main.py mcp`, `mcp_serve.py`, `tools/mcp_tool.py` | `crates/hermes-integrations` owns MCP server/tool/event contract snapshots. Live FastMCP stdio serving, event bridge, session polling, approval response, and dynamic tool discovery remain Python. | `python-primary`, `external-boundary candidate`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.4` | Rust MCP server smoke for list/read/send/poll/wait/approval tools plus dynamic MCP tool registration against configured servers. |
| RL CLI and RL training tools | `rl_cli.py`, `tools/rl_training_tool.py`, `environments/` | `crates/hermes-integrations` owns CLI/default/toolset contract snapshots. Tinker-Atropos integration, environments, async training tools, and inference remain Python. | `python-primary`, `external-boundary candidate`, `deletion-blocker` | `hermes-fpr.7`, `hermes-fpr.4` | Rust runtime smoke for listing/selecting environments, config editing, start/status/stop/results, and a minimal training/inference fixture or signed external-boundary decision. |
| Tool registry, dispatch, approvals, and handlers | `model_tools.py`, `toolsets.py`, `tools/registry.py`, `tools/` | `crates/hermes-tools` owns registry/toolset/schema, dispatch/error, safety, and a native file-handler slice. Terminal/process, browser/web, delegate, MCP, memory/todo, media, messaging, platform, and environment handlers remain Python. | `contract-tested`, mostly `python-primary`, `deletion-blocker` | `hermes-fpr.4` | Core tool E2E suite with Rust dispatch and Rust handlers, plus explicit external-boundary sign-off for any handler family not ported. |
| State store | `hermes_state_factory.py`, `hermes_state_rust.py`, `hermes_state.py` | `crates/hermes-state` owns a tested SQLite store, probe, and daemon. Production call sites reach it through the Python factory and adapter. | `rust backend partial`, `python-primary integration` | `hermes-fpr.2`, `hermes-fpr.6`, `hermes-fpr.7` | Rust-default application smoke that opens clean and migrated profiles without Python adapters, runs session create/append/search/list/delete, and validates rollback diagnostics. |
| Skills | `skills/`, `optional-skills/`, `agent/skill_commands.py`, `hermes_cli/skills_hub.py`, `tools/skills_tool.py`, `tools/skill_manager_tool.py` | Built-in and optional skills are mostly data files, but discovery, install/update/audit/snapshot, slash injection, config prompting, provenance, and management tools remain Python. | `python-primary`, `external-boundary candidate`, `deletion-blocker` | `hermes-fpr.6`, `hermes-fpr.8`, `hermes-fpr.4` | Rust skill smoke for built-in and optional skill discovery, install/uninstall/update/audit/snapshot, slash invocation, config injection, provenance, and no prompt-cache-breaking mutation. |
| General plugins and plugin CLI commands | `hermes_cli/plugins.py`, `plugins/`, pip entry point group `hermes_agent.plugins` | `crates/hermes-integrations` owns plugin facade/hook/manifest/discovery contract snapshots. `crates/hermes-cli` now owns bounded `plugins list/enable/disable` config behavior. Actual Python plugin modules may register tools, hooks, platforms, skills, CLI commands, dashboard APIs, providers, and memory backends. | `python-primary`, `external-boundary candidate`, `deletion-blocker for repo-shipped Python plugins only` | `hermes-fpr.8`, `hermes-fpr.5`, `hermes-fpr.6` | Rust smoke for plugin list/enable/disable plus repo-shipped plugin review. Python plugin ABI compatibility is not required unless an explicit RPC/IPC contract already exists; external user/pip Python plugins can be converted to Rust on demand and are not a Python source-removal blocker. |
| Install, update, uninstall, and packaging | `pyproject.toml`, `scripts/install.sh`, `hermes_cli/main.py update`, `hermes_cli/uninstall.py` | No Rust-owned top-level installer or command selection path. Existing install links the Python console script. | `python-primary`, `deletion-blocker` | `hermes-fpr.2`, `hermes-fpr.10` | Clean install, update, uninstall, rollback, and migrated-profile smoke with Rust as default and Python fallback only when explicitly selected during rollout. |

## Rust Artifact Inventory

| Rust artifact | Current scope | Production cutover gap |
| --- | --- | --- |
| `crates/hermes-state` | SQLite state store, search/title/schema ops, subprocess probe, and Unix-socket daemon. | Remove Python adapter dependency from application entry points and make Rust state the default in clean/migrated runtime smoke tests. |
| `crates/hermes-agent-core` | Message/tool/budget/compression/provider wire models, canned loop, replay fixtures. | Add real provider HTTP clients, streaming execution, credentials, fallback, interrupts, persistence, lifecycle hooks, and real tool dispatch. |
| `crates/hermes-cli` | Slash/command registry, selected setup/auth planning, display/log/status contracts. | Own top-level command dispatch, runtime selector, interactive/non-interactive CLI behavior, setup/auth/config/profile/log/skin/update flows. |
| `crates/hermes-config` | HERMES_HOME/profile/config/env-bridge contract probes. | Become the production config loader for all Rust-owned entry points without Python loaders. |
| `crates/hermes-gateway` | Session guard, slash routing, streaming/delivery contracts, adapter trait, in-memory smoke adapter. | Own `GatewayRunner`, platform lifecycles, approvals, token locks, delivery, restart/update, queue/status/stop behavior. |
| `crates/hermes-tools` | Toolset/schema registry, dispatch envelopes, safety guardrails, native file-handler slice. | Port remaining handler families or move them behind signed external boundaries that survive Python deletion. |
| `crates/hermes-tui-gateway` | JSON-RPC protocol and event snapshots. | Implement live TUI backend runtime with Rust sessions, agent, slash, approval, completion, and tool streams. |
| `crates/hermes-dashboard` | FastAPI route/auth/WebSocket/embedded-chat contract snapshots. | Replace Python FastAPI runtime or explicitly gate a non-Python dashboard server boundary. |
| `crates/hermes-acp` | ACP capability/method/session/permission/tool-rendering contracts. | Implement live ACP stdio server and session/agent/tool integration. |
| `crates/hermes-integrations` | Cron, batch, MCP, RL, and plugin boundary snapshots. | Replace or gate live integration runtimes and any repo-owned plugin/provider surfaces. |

## Deletion Blockers By Workstream

| Workstream | Deletion blocker |
| --- | --- |
| `hermes-fpr.2` | Rust launcher/runtime selector exists and is tested. Remaining risk is broader packaging: pip/editable-package console scripts remain Python fallback until Rust packaging replaces them or they are intentionally retained as an explicit rollback path. |
| `hermes-fpr.3` | Live agent execution remains Python: model HTTP, streaming, credentials, fallback, callbacks, budgets, interrupts, compression, persistence, and hooks are not Rust-primary. |
| `hermes-fpr.4` | Most side-effect-heavy tools remain Python handlers. Only a low-risk file-handler slice is native Rust. |
| `hermes-fpr.5` | Gateway runner and platform adapters remain Python despite tested Rust contracts. |
| `hermes-fpr.6` | CLI setup/auth/config/profile/log/skin/update/session workflows remain Python runtime code. |
| `hermes-fpr.7` | TUI gateway, dashboard backend, ACP, cron, batch, MCP, and RL are contract-tested but Python-primary. |
| `hermes-fpr.8` | Skills discovery/install/snapshot/slash injection and repo-shipped plugin/provider/platform/dashboard APIs still depend on Python. External user/pip Python plugin ABI compatibility is not required unless an explicit RPC/IPC contract exists. |
| `hermes-fpr.9` | Shadow harness exists in `scripts/rust_shadow_diff.py` and is CI-gated by `tests/parity/test_shadow_diff.py`; remaining risk is expanding coverage as `hermes-fpr.4`-`.8` move from contracts to production Rust runtimes. |
| `hermes-fpr.10` | Python source removal is blocked until every prior row is `default` or explicitly deferred with owner sign-off. |

## Coverage Checklist

| Required surface | Covered by inventory row |
| --- | --- |
| CLI | Installed `hermes` command and top-level dispatch; Classic interactive and single-query chat CLI |
| Agent | Direct `hermes-agent` command |
| Gateway | Gateway command and service lifecycle; Gateway platform adapters |
| TUI | TUI backend for `hermes --tui` |
| Dashboard | Dashboard backend and embedded chat |
| ACP | ACP command and stdio server |
| Cron | Cron CLI, scheduler, and cron tool |
| Batch | Batch runner |
| MCP | MCP server and dynamic MCP tools |
| Tools | Tool registry, dispatch, approvals, and handlers |
| Skills | Skills |
| Plugins | General plugins and plugin CLI commands |

## Next Work After This Audit

With `hermes-fpr.1` complete, the next unblockable work is implementation, not
planning:

1. `hermes-fpr.2`: create a Rust-owned `hermes` launcher/runtime selector while
   preserving explicit Python fallback during rollout.
2. `hermes-fpr.3`: make the Rust agent loop execute a real mock-provider E2E
   conversation with production-shaped credentials, streaming, tool dispatch,
   persistence, interrupts, budgets, and compression.
3. `hermes-fpr.4`: expand native Rust tool handlers or mark each remaining
   handler family as an explicit external boundary with tests and sign-off.

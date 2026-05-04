# Hermes Agent Codebase Audit Report

Date: 2026-05-04
Repository: `/Users/jordanh/Src/hermes-agent`
Package version observed: `hermes-agent` 0.12.0 from `pyproject.toml`

## Audit Scope

This report is based on a repository-wide source inventory plus deep inspection of the load-bearing runtime paths: the agent loop, provider/client selection, tool registry and dispatcher, CLI, TUI gateway, messaging gateway, dashboard server, session store, plugins, memory providers, execution environments, approvals, checkpoints, config, and tests.

Evidence gathered:

- Enumerated tracked files and source tree with `rg --files`, `git ls-files`, `find`, and targeted AST/import inventory.
- Counted roughly 2,953 tracked paths and about 1,202,750 tracked lines. Python dominates runtime code: about 1,384 tracked `.py` files. TypeScript/JavaScript accounts for about 380 tracked frontend/TUI/docs tooling files.
- Imported the live tool registry with Python 3.11 through `uv run python` and captured the actual registered tool names and toolsets.
- Imported `hermes_cli.commands.COMMAND_REGISTRY` and captured the canonical slash command surface.
- Inspected package manifests for Python, TUI, dashboard, and docs dependencies.
- Inspected representative tests across agent, CLI, gateway, ACP, cron, dashboard/TUI, tools, memory, and plugins.

Important honesty note: the checkout contains over one million lines including documentation, tests, shipped skills, optional skills, website content, and frontend assets. I did not pretend to memorize every byte. The report maps the full tree and all primary feature surfaces, and it deep-reads the code that actually defines runtime behavior. A literal line-by-line signoff of every test fixture, generated asset, and documentation page would be a separate multi-day review.

## Executive Summary

Hermes Agent is a multi-transport AI agent runtime. It can run as an interactive CLI, an Ink-based terminal UI, a web dashboard, an ACP/editor adapter, a scheduled cron worker, a batch runner, or a messaging gateway for platforms such as Telegram, Discord, Slack, WhatsApp, Matrix, Mattermost, Signal, Feishu, WeCom, Weixin, SMS, email, webhook/API server, Home Assistant, Yuanbao, QQBot, and plugin platforms.

The core is `AIAgent` in `run_agent.py`. It builds provider clients, composes prompts, manages session state, executes model tool calls, handles context compression, integrates memory/context-engine plugins, and tracks token/cost/runtime metadata. Tools are registered through `tools/registry.py`, exposed through `model_tools.py`, grouped by `toolsets.py`, and executed through a shared dispatcher. Most user-visible transports eventually call the same `AIAgent.run_conversation()` path.

The project is not a toy. It has mature subsystems for provider routing, tool safety, profile isolation, persistent sessions, prompt caching, context compression, credentials, plugin loading, gateway delivery, dashboard APIs, and a large test suite. The main quality issue is structural: several files have become operational centers of gravity (`run_agent.py`, `gateway/run.py`, `cli.py`, `hermes_cli/main.py`, `tui_gateway/server.py`). These files contain too much policy, transport glue, state management, command dispatch, and error recovery in one place. The code works hard to defend against real edge cases, but the long-term maintainability risk is high unless these files are decomposed behind shared services.

Overall grade: B+.

Production capability is high, test coverage is broad, and the architectural primitives are mostly good. Maintainability is pulled down by oversized orchestrators, duplicated slash-command behavior across transports, heavy dynamic import behavior, and a very large config surface.

## Application Purpose

Hermes is an extensible local-first AI agent platform. Its main promise is to let a user interact with a model that can:

- Keep persistent conversation sessions.
- Use local and remote tools.
- Read, write, search, patch, and execute code.
- Automate browsers and web research.
- Use multimodal inputs and generation providers.
- Delegate work to subagents.
- Operate from CLI, TUI, dashboard, editor ACP clients, or chat platforms.
- Schedule recurring tasks.
- Keep memory across sessions through built-in and plugin memory providers.
- Load task-specific skills from local, bundled, optional, or external skill directories.
- Protect dangerous operations with approvals, checkpoints, profile isolation, and command guardrails.

## Repository Shape

The highest-value runtime entry points are:

| Area | Files | Role |
| --- | --- | --- |
| Agent loop | `run_agent.py` | `AIAgent`, provider client setup, model loop, prompt cache, tool execution, compression, memory/context integration |
| Tool dispatch | `model_tools.py`, `tools/registry.py`, `toolsets.py` | Tool discovery, schemas, availability checks, dispatcher, toolset grouping |
| Classic CLI | `cli.py`, `hermes_cli/main.py`, `hermes_cli/commands.py` | argparse entry point, interactive Rich/prompt_toolkit CLI, slash command registry and handlers |
| TUI | `ui-tui/src/*`, `tui_gateway/server.py` | Ink React UI over stdio JSON-RPC to Python gateway and `AIAgent` |
| Dashboard | `hermes_cli/web_server.py`, `web/src/*` | FastAPI REST/WS backend, Vite React dashboard, PTY bridge for embedded TUI |
| Messaging gateway | `gateway/run.py`, `gateway/platforms/*`, `gateway/session.py` | Platform adapters, session routing, auth/allowlists, command handling, agent cache |
| Persistent state | `hermes_state.py` | SQLite sessions/messages, FTS5/trigram search, compression lineage |
| Config/profile/logging | `hermes_cli/config.py`, `hermes_constants.py`, `hermes_logging.py` | Config defaults, env metadata, profile-aware paths, logging |
| Memory/context | `agent/memory_manager.py`, `agent/memory_provider.py`, `agent/context_engine.py`, `plugins/memory/*`, `plugins/context_engine/*` | Memory provider abstraction and context-engine plugin surface |
| Provider routing | `hermes_cli/runtime_provider.py`, `hermes_cli/providers.py`, `agent/auxiliary_client.py`, `agent/model_metadata.py`, `agent/credential_pool.py` | Runtime credentials, provider identity, model metadata/context/pricing, fallback and auxiliary LLMs |
| Safety | `tools/approval.py`, `tools/checkpoint_manager.py`, `agent/tool_guardrails.py` | Dangerous command detection, approval queues, smart approvals, filesystem checkpoints, loop guardrails |
| Execution environments | `tools/terminal_tool.py`, `tools/environments/*`, `tools/code_execution_tool.py`, `tools/process_registry.py` | Local/Docker/SSH/Modal/Daytona/Singularity/Vercel execution, background process tracking, code sandbox RPC |
| Automation | `cron/*`, `batch_runner.py`, `trajectory_compressor.py`, `environments/*` | Scheduled jobs, batch datasets, trajectory compression, RL training environments |
| Plugins | `hermes_cli/plugins.py`, `plugins/*` | Generic plugin manager, lifecycle hooks, plugin tools, memory/image/context/platform providers, dashboard extensions |
| Tests | `tests/*`, `ui-tui/src/__tests__/*` | Broad pytest and Vitest coverage |
| Docs | `website/docs/*`, `README.md`, `AGENTS.md` | User/developer documentation |

Approximate line concentration from the inventory:

| Path | Approx. lines | Notes |
| --- | ---: | --- |
| `tests/` | 334,663 | Largest area; strong signal of regression coverage |
| `skills/` | 189,921 | Built-in skill library |
| `website/` | 156,558 | Docusaurus docs and content |
| `hermes_cli/` | 73,324 | CLI config/auth/dashboard/main modules |
| `gateway/` | 72,124 | Messaging runtime and platform adapters |
| `ui-tui/` | 63,186 | Ink TUI and tests |
| `tools/` | 62,786 | Tool implementations and execution backends |
| `optional-skills/` | 61,729 | Installable heavy/niche skills |
| `plugins/` | 39,252 | Memory, image, observability, platform, dashboard plugins |
| `agent/` | 33,495 | Provider, memory, prompt, compression, guardrails |
| `web/` | 24,179 | Dashboard frontend |
| `run_agent.py` | 14,200 | Core agent orchestrator |
| `gateway/run.py` | 14,384 | Messaging orchestrator |
| `cli.py` | 12,096 | Classic CLI orchestrator |
| `hermes_cli/main.py` | 10,479 | argparse app and setup/auth/profile/dashboard commands |
| `tui_gateway/server.py` | 6,165 | JSON-RPC bridge from Ink/browser to Python agent |

## Runtime Architecture

### Core agent turn

1. A transport receives user input: CLI, TUI, gateway, ACP, cron, dashboard/API, batch runner.
2. The transport creates or resumes an `AIAgent`.
3. `AIAgent.run_conversation()` builds or reuses the session system prompt, context files, skill index, memory context, prefill messages, and conversation history.
4. The provider client is selected from explicit credentials, config, OAuth/device-code credentials, credential pools, custom providers, or fallback chain.
5. `model_tools.get_tool_definitions()` exposes enabled tools after registry availability checks.
6. The model response is normalized. If it has tool calls, `_execute_tool_calls()` dispatches them through `model_tools.handle_function_call()` or through agent-level interceptors.
7. Tool results are appended, persisted, truncated as needed, and may update local state, memory, checkpoints, process registries, or gateway notifications.
8. If the model returns final content, the answer is persisted and returned to the transport.
9. If context is too large, `agent/context_compressor.py` summarizes middle turns, preserves head/tail context, and `hermes_state.py` may split the session lineage.

### CLI

`hermes_cli/main.py` pre-parses profile flags, applies `HERMES_HOME`, loads config/env, builds argparse subcommands, and starts `HermesCLI` in `cli.py` for interactive chat. `HermesCLI.process_command()` resolves slash commands through `hermes_cli/commands.py` and contains transport-specific command handlers for sessions, tools, model switching, skills, cron, rollback, browser, background tasks, voice, and usage.

Common libraries: `rich`, `prompt_toolkit`, `pyperclip`, `PyYAML`, Python stdlib `argparse`, `sqlite3`, `logging`.

### TUI

`ui-tui/src/gatewayClient.ts` spawns Python `-m tui_gateway.entry` and speaks newline JSON-RPC over stdio. `ui-tui/src/app.tsx` and components render transcript, composer, prompts, approvals, session picker, model picker, tool progress, todo panel, and overlays. `tui_gateway/server.py` owns Python-side sessions, lazy `AIAgent` construction, slash command execution through a worker subprocess, long-running request dispatch, approvals, session finalization, and events back to Ink.

Common libraries: `ink`, React, `nanostores`, Vitest, local `@hermes/ink`.

### Dashboard

`hermes_cli/web_server.py` is a FastAPI app serving REST, WebSockets, plugin APIs, and the Vite SPA. The dashboard has APIs for status, config/defaults/schema, env secrets, OAuth providers, sessions/search/messages, logs, cron jobs, profiles, skills, toolsets, analytics, model options/assignment, update/restart actions, themes, and plugin manifests.

The `/chat` page deliberately embeds the real `hermes --tui` through `/api/pty`, with xterm.js rendering PTY bytes in `web/src/pages/ChatPage.tsx`. A sidecar JSON-RPC socket (`/api/ws`) and event broadcast sockets (`/api/pub`, `/api/events`) let React sidebar widgets inspect activity without reimplementing chat.

Common libraries: FastAPI, Uvicorn, pydantic, ptyprocess, React 19, Vite, xterm.js, Tailwind, lucide-react, `@nous-research/ui`.

### Messaging gateway

`gateway/run.py` is the async platform orchestrator. It loads raw gateway config, starts adapters, manages session stores, pairing/allowlists, active-session guards, approval commands, background task queues, auto-continue, model/reasoning/session overrides, AIAgent LRU cache, restart/update flows, process watchers, home channels, voice input/output, and slash command dispatch.

Platform adapter files live in `gateway/platforms/`. Built-in adapters include Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Mattermost, email, SMS, DingTalk, WeCom, Weixin, Feishu, Feishu comments, QQBot, BlueBubbles, Home Assistant, webhook, API server, and Yuanbao. Plugin platform adapters are registered through `plugins/platforms/irc` and `plugins/platforms/teams`.

Common libraries vary by adapter: `python-telegram-bot`, `discord.py`, `slack_bolt`, `slack_sdk`, `aiohttp`, email/IMAP/SMTP libraries, Matrix/Mattermost/Signal clients, HTTPX, platform-specific SDKs.

### ACP/editor integration

`acp_adapter/server.py` implements the Agent Communication Protocol server for editor clients such as Zed/VS Code/JetBrains integrations. It supports initialize/authenticate, new/load/resume/fork/list/cancel sessions, prompts with tool progress and permission bridging, history replay, model/mode/config updates, queued prompts, compression, context usage updates, and session MCP server registration.

Common libraries: `agent-client-protocol`, asyncio, ThreadPoolExecutor, SQLite session store, Hermes approval callbacks.

### Cron and batch

`cron/jobs.py` stores scheduled jobs and rewrites skill references. `cron/scheduler.py` ticks and executes due jobs. `tools/cronjob_tools.py` exposes scheduling to the model. `batch_runner.py` runs prompt datasets in resumable batches and extracts tool/reasoning stats. `trajectory_compressor.py` summarizes saved trajectories for evaluation/training.

Common libraries: `croniter`, multiprocessing/concurrent futures, SQLite/JSON, OpenAI-compatible clients.

## Feature Map

| Feature | Source files | Common libraries / dependencies | Notes |
| --- | --- | --- | --- |
| Core agent conversation | `run_agent.py` | OpenAI SDK, Anthropic SDK, boto3/Bedrock path, httpx, tenacity-style retries, local helpers | The largest and most critical file. Handles provider setup, request assembly, tool calls, compression, memory, fallback, streaming, usage. |
| Provider identity and runtime credentials | `hermes_cli/providers.py`, `hermes_cli/runtime_provider.py`, `agent/credential_pool.py`, `agent/credential_sources.py`, `agent/auxiliary_client.py` | OpenAI/Anthropic SDKs, httpx, keyring/OAuth helpers, models.dev data | Supports direct providers, aggregators, OAuth/device-code providers, custom providers, credential pool strategies, auxiliary task models. |
| Model metadata/context/pricing | `agent/model_metadata.py`, `agent/models_dev.py`, `hermes_cli/model_catalog.py`, `agent/usage_pricing.py` | httpx, cached JSON | Resolves context length from config, provider API, OpenRouter, models.dev, local server probes, and static fallbacks. |
| Prompt assembly | `agent/prompt_builder.py`, `agent/prompt_caching.py`, `agent/skill_utils.py` | PyYAML, filesystem scanning | Loads `SOUL.md`, `HERMES.md`, `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, skills index, env hints, memory/search guidance; includes prompt-injection scanning for context files. |
| Session persistence | `hermes_state.py` | SQLite, FTS5, WAL | Stores sessions/messages/reasoning/tool calls, session titles, compression lineage, FTS5 and trigram search. Declarative column reconciliation is a strong design choice. |
| Context compression | `agent/context_compressor.py`, `agent/context_engine.py`, `plugins/context_engine/*` | Auxiliary LLM calls, token estimation | Compresses middle turns, preserves head/tail, summarizes old tool outputs, guards against losing active task, supports plugin context engines. |
| Tool registration and dispatch | `tools/registry.py`, `model_tools.py`, `toolsets.py` | AST scanning, importlib, JSON schema | Registry imports only modules with top-level `registry.register()`, caches schemas, dispatches sync/async handlers, applies plugin hooks and schema post-processing. |
| File operations | `tools/file_tools.py` | pathlib, shell/file operation helpers | `read_file`, `write_file`, `patch`, `search_files`; includes read dedup, staleness checks, sensitive path checks, max result caps. |
| Terminal/process execution | `tools/terminal_tool.py`, `tools/process_registry.py`, `tools/environments/*` | subprocess, Docker CLI, SSH, Modal SDK, Daytona SDK, Vercel SDK, Singularity/Apptainer | Supports local, Docker, SSH, Modal, managed Modal, Daytona, Singularity, Vercel. Background process registry survives gateway restarts through checkpoints. |
| Code execution sandbox | `tools/code_execution_tool.py` | Python subprocess/RPC, generated Hermes tools module | Runs code with selected sandbox tools, ships helper modules, supports remote/staged execution and RPC calls back to allowed tools. |
| Browser automation | `tools/browser_tool.py`, `tools/browser_cdp_tool.py`, `tools/browser_dialog_tool.py`, `tools/browser_providers/*` | `agent-browser`, Playwright/CDP-style sidecar, Browserbase/Firecrawl/browser-use providers | Navigate, snapshot, click, type, scroll, console, screenshots, vision over screenshots, CDP connection and cleanup. |
| Web research | `tools/web_tools.py` | Exa, Firecrawl, Parallel Web, HTTP extraction | `web_search`, `web_extract` with provider availability checks. |
| Vision/video | `tools/vision_tools.py` | OpenAI-compatible vision, image/video download/resize, Pillow-like image handling | `vision_analyze`, `video_analyze`, validates URLs, MIME, resizing, download retries. |
| Image generation | `tools/image_generation_tool.py`, `agent/image_gen_provider.py`, `plugins/image_gen/*` | fal-client, OpenAI image APIs, xAI APIs, managed FAL gateway | Built-in FAL path plus plugin providers for OpenAI, OpenAI Codex, and xAI. |
| TTS/STT/voice | `tools/tts_tool.py`, gateway voice paths | edge-tts, platform voice adapters, optional STT integrations | Text-to-speech tool and gateway voice modes. |
| Skills | `tools/skills_tool.py`, `tools/skill_manager_tool.py`, `agent/skill_commands.py`, `skills/*`, `optional-skills/*` | PyYAML, filesystem scan | 138 `SKILL.md` files counted across built-in and optional skill surfaces. Slash commands load skills as user messages to preserve prompt cache. |
| Memory | `tools/memory_tool.py`, `agent/memory_manager.py`, `agent/memory_provider.py`, `plugins/memory/*` | Provider SDKs/HTTPX/SQLite depending on plugin | Local memory plus provider plugins: honcho, mem0, supermemory, byterover, hindsight, holographic, openviking, retaindb. |
| Session search | `tools/session_search_tool.py`, `hermes_state.py` | SQLite FTS5 | Lets the agent search past conversations; prompt advises when to use it. |
| Delegation/subagents | `tools/delegate_tool.py`, `tools/mixture_of_agents_tool.py` | ThreadPoolExecutor, AIAgent child construction | Parallel child agents with depth/concurrency controls, inherited MCP toolsets, approval handling, progress callbacks. |
| Cron scheduling | `cron/*`, `tools/cronjob_tools.py`, `hermes_cli/main.py`, `hermes_cli/web_server.py` | croniter, SQLite/JSON, API routes | Create/list/edit/pause/resume/trigger jobs from CLI, dashboard, or model tool. |
| Kanban/collaboration | `tools/kanban_tools.py`, `plugins/kanban/dashboard/*`, `hermes_cli/kanban*` | SQLite, FastAPI plugin router, dashboard plugin | Multi-profile task board with show/create/complete/block/comment/link/heartbeat tools and dashboard API. |
| Messaging | `tools/send_message_tool.py`, `gateway/*` | Adapter-specific SDKs | Agent can send messages through the active gateway/router. |
| Gateway platform runtime | `gateway/run.py`, `gateway/platforms/*`, `gateway/platform_registry.py` | asyncio, adapter SDKs, HTTPX | Platform adapters normalize inbound events and outbound delivery. |
| Dashboard | `hermes_cli/web_server.py`, `web/src/*`, `plugins/*/dashboard` | FastAPI, React, Vite, xterm, Tailwind | Config/model/session/logs/analytics/cron/profiles/skills/plugins UI; real TUI embedded via PTY. |
| ACP | `acp_adapter/*` | agent-client-protocol | Editor protocol server with permission bridge and MCP support. |
| RL training | `tools/rl_training_tool.py`, `environments/*` | Atropos/RL environment dependencies | Tool surface for selecting envs, editing configs, starting/stopping training, reading results. |
| Plugins | `hermes_cli/plugins.py`, `plugins/*` | importlib.metadata entry points, FastAPI routers, tool registry | Generic hooks, CLI commands, tools, platform adapters, dashboard extensions, observability. |
| Approvals and safety | `tools/approval.py`, `agent/tool_guardrails.py`, `tools/tirith_security.py` | Contextvars, threading, auxiliary LLM for smart approvals | Dangerous command detection, hardline blocks, gateway approval queues, session/permanent allowlist, smart approvals. |
| Checkpoints/rollback | `tools/checkpoint_manager.py`, `cli.py`, `gateway/run.py` | Git shadow repos | Transparent filesystem snapshots before mutations, list/diff/restore, startup pruning. |
| Profiles and paths | `hermes_constants.py`, `hermes_cli/main.py`, `hermes_cli/config.py` | pathlib, env vars | Profile-safe `HERMES_HOME`, display paths, config/env/skills dirs. |
| Logging and diagnostics | `hermes_logging.py`, `hermes_cli/debug.py`, `hermes_cli/web_server.py` | Rotating file handlers, Sentry optional, upload helpers | Profile-aware logs, session context in records, dashboard log API, debug report command. |

## Registered Tool Surface

The live registry under Python 3.11 reported these built-in and bundled plugin tools:

| Toolset | Tools |
| --- | --- |
| `browser` | `browser_back`, `browser_click`, `browser_console`, `browser_get_images`, `browser_navigate`, `browser_press`, `browser_scroll`, `browser_snapshot`, `browser_type`, `browser_vision` |
| `browser-cdp` | `browser_cdp`, `browser_dialog` |
| `clarify` | `clarify` |
| `cronjob` | `cronjob` |
| `delegation` | `delegate_task` |
| `discord` | `discord` |
| `discord_admin` | `discord_admin` |
| `code_execution` | `execute_code` |
| `feishu_doc` | `feishu_doc_read` |
| `feishu_drive` | `feishu_drive_add_comment`, `feishu_drive_list_comment_replies`, `feishu_drive_list_comments`, `feishu_drive_reply_comment` |
| `homeassistant` | `ha_call_service`, `ha_get_state`, `ha_list_entities`, `ha_list_services` |
| `image_gen` | `image_generate` |
| `kanban` | `kanban_block`, `kanban_comment`, `kanban_complete`, `kanban_create`, `kanban_heartbeat`, `kanban_link`, `kanban_show` |
| `memory` | `memory` |
| `moa` | `mixture_of_agents` |
| `file` | `patch`, `read_file`, `search_files`, `write_file` |
| `terminal` | `process`, `terminal` |
| `rl` | `rl_check_status`, `rl_edit_config`, `rl_get_current_config`, `rl_get_results`, `rl_list_environments`, `rl_list_runs`, `rl_select_environment`, `rl_start_training`, `rl_stop_training`, `rl_test_inference` |
| `messaging` | `send_message` |
| `session_search` | `session_search` |
| `skills` | `skill_manage`, `skill_view`, `skills_list` |
| `spotify` | `spotify_albums`, `spotify_devices`, `spotify_library`, `spotify_playback`, `spotify_playlists`, `spotify_queue`, `spotify_search` |
| `tts` | `text_to_speech` |
| `todo` | `todo` |
| `video` | `video_analyze` |
| `vision` | `vision_analyze` |
| `web` | `web_extract`, `web_search` |
| `hermes-yuanbao` | `yb_query_group_info`, `yb_query_group_members`, `yb_search_sticker`, `yb_send_dm`, `yb_send_sticker` |

## Slash Command Surface

`hermes_cli/commands.py` centralizes the canonical command definitions. Downstream consumers derive CLI autocomplete/help, gateway known commands, gateway help, Telegram command menus, and Slack subcommand maps from this registry.

Command categories observed:

- Session: `/new`, `/clear`, `/redraw`, `/history`, `/save`, `/retry`, `/undo`, `/title`, `/branch`, `/compress`, `/rollback`, `/snapshot`, `/stop`, `/approve`, `/deny`, `/background`, `/agents`, `/queue`, `/steer`, `/goal`, `/status`, `/sethome`, `/resume`, `/restart`.
- Configuration: `/config`, `/model`, `/personality`, `/statusbar`, `/verbose`, `/footer`, `/yolo`, `/reasoning`, `/fast`, `/skin`, `/indicator`, `/voice`, `/busy`.
- Tools and skills: `/tools`, `/toolsets`, `/skills`, `/cron`, `/curator`, `/kanban`, `/reload`, `/reload-mcp`, `/reload-skills`, `/browser`, `/plugins`.
- Info: `/profile`, `/gquota`, `/commands`, `/help`, `/usage`, `/insights`, `/platforms`, `/copy`, `/paste`, `/image`, `/update`, `/debug`.
- Exit: `/quit`.

The central registry is good architecture. The duplicated handler logic across `cli.py`, `gateway/run.py`, `tui_gateway/server.py`, and ACP command paths is the remaining architectural debt.

## Plugin Surfaces

`hermes_cli/plugins.py` supports:

- Lifecycle hooks: pre/post tool calls, pre/post LLM calls, session start/end/finalize, approval hooks, gateway dispatch hooks, and related extension points.
- Tool registration through `PluginContext.register_tool()`, delegated to `tools.registry`.
- Platform registration through `PluginContext.register_platform()`, delegated to `gateway/platform_registry.py`.
- Dashboard plugins with manifest and optional FastAPI router mounting under `/api/plugins/<name>/`.
- CLI subcommands through plugin-provided `register_cli()` functions and argparse wiring.
- Discovery from bundled `plugins/`, user `~/.hermes/plugins/`, repo-local `.hermes/plugins/`, and Python entry points.

Bundled plugin categories observed:

- Memory providers: `honcho`, `mem0`, `supermemory`, `byterover`, `hindsight`, `holographic`, `openviking`, `retaindb`.
- Image generation providers: `openai`, `openai-codex`, `xai`.
- Platform adapters: `irc`, `teams`.
- General tools/integrations: `spotify`, `google_meet`, `disk-cleanup`, `observability/langfuse`.
- Dashboard extensions: example dashboard, kanban, achievements, cockpit-style plugin manifests.

The plugin boundary is one of the better parts of the architecture. The project has also documented a healthy rule: plugins should not hardcode themselves into core files; new capabilities should extend the generic plugin surface.

## State, Memory, and Context

`hermes_state.py` is strong. It uses SQLite WAL, schema versioning, declarative column reconciliation, FTS5, trigram FTS for CJK substring search, transaction retries with jitter, session title uniqueness, compression lineage projection, and safe export/prune/vacuum paths. This is production-minded state code.

Memory has two layers:

- Local/session memory guidance and a `memory` tool.
- External provider plugins through `agent/memory_provider.py` and `plugins/memory/*`.

`agent/memory_manager.py` scrubs sensitive context and builds memory context blocks. Providers implement lifecycle hooks such as `sync_turn`, `prefetch`, `shutdown`, and optional setup/post-setup hooks.

Context compression is also mature. `agent/context_compressor.py` protects head/tail context, summarizes old tool outputs before LLM summarization, scales summary budget with context length, redacts secrets, tracks summary failures, falls back from failed auxiliary summary models, and specifically guards against losing the active user task when tool-call group alignment shifts the compression boundary.

## Security and Safety

Safety features are real and layered:

- `tools/approval.py` is the single source of truth for dangerous command detection, per-session approval state, hardline blocks, gateway approval queues, CLI approval prompts, permanent allowlists, smart approvals, and approval lifecycle hooks.
- `tools/terminal_tool.py` routes all pre-exec terminal checks through approvals and validates workdirs, sudo handling, background hints, env import, and persistent environment cleanup.
- `tools/checkpoint_manager.py` uses shadow git repositories under `HERMES_HOME` to snapshot working directories before mutations and supports list/diff/restore.
- `agent/tool_guardrails.py` detects tool-call loops and repeated failures.
- `hermes_constants.py` and profile initialization in `hermes_cli/main.py` keep paths profile-aware.
- `hermes_cli/config.py` distinguishes config values from secret `.env` values, applies secure permissions, and validates config.
- Gateway adapters use allowlists/pairing and platform-specific guards.

Residual risk: the project executes real shell commands, browser sessions, cloud sandboxes, and messaging actions. The approval and checkpoint systems are necessary but not sufficient as a formal sandbox boundary. Local mode is intentionally powerful.

## Test Coverage

Observed Python test count: 903 `test_*.py` files. The suite is broad and includes:

- Agent/provider routing, compression, memory, prompt caching, model metadata, auxiliary clients, error classification, usage pricing.
- CLI commands, approval UI, browser connect, reload skills/MCP, manual compression, model switching, worktree safety.
- Gateway platform adapters and command behavior across Discord, Telegram, Slack, Feishu, DingTalk, Home Assistant, Matrix-like flows, API server, background tasks, approvals, queue/busy modes, voice, restart/update.
- ACP protocol, permissions, tools, MCP e2e, session handling.
- Cron scheduler/jobs and execution paths.
- Plugins and memory providers.
- Tool behavior, terminal/process registries, file tools, checkpoints, browser, skills.
- Dashboard/TUI gateway and frontend behavior.
- TUI Vitest coverage for slash parity, terminal parity, viewport/scrolling, markdown, messages, queues, session lifecycle, RPC, state isolation.

The required runner is `scripts/run_tests.sh`, which is a good sign: it enforces hermetic CI parity, unsets credential vars, fixes timezone/locale, and limits xdist workers.

## Code Quality Grades

| Area | Grade | Rationale |
| --- | --- | --- |
| Core agent loop | B | Extremely capable and handles many real provider/tool edge cases. Too much responsibility in one 14k-line file. |
| Tool registry/dispatcher | A- | Clear registry, availability checks, schema caching, dynamic plugin/MCP support. Some global state and import side effects add complexity. |
| Tool implementations | B+ | Tools are practical and defensive. Terminal/browser/delegation/code execution are large but show real operational hardening. |
| Session DB | A- | Strong SQLite design, FTS, migrations, contention handling, compression lineage. |
| Provider/runtime config | B+ | Handles a difficult provider landscape. Complexity is unavoidable but spread across config, providers, runtime provider, credential pool, auxiliary client. |
| CLI | B | Feature-rich and usable. `cli.py` is too large and duplicates command semantics with gateway/TUI. |
| TUI | A- | Good split between Ink UI and Python runtime. Tests are healthy. Gateway server is large but conceptually coherent. |
| Dashboard | B+ | Rich FastAPI/Vite dashboard. Correctly embeds real TUI instead of forking chat UX. Web server file is oversized. |
| Messaging gateway | B | Impressive platform breadth and operational detail. `gateway/run.py` is a large monolith with many intertwined state machines. |
| Plugins | A- | Good generic extension story; memory/image/platform/dashboard plugins are cleanly discoverable. |
| Safety | B+ | Approval/checkpoint/profile systems are serious. Risk remains because local agent execution is inherently high-impact. |
| Tests | A- | Very broad test surface. Full quality depends on keeping the canonical runner fast enough for regular use. |
| Documentation | B+ | Large Docusaurus docs and in-repo guidance. Volume is high; keeping docs synchronized with rapid code changes is the risk. |

Overall: B+.

## Strengths

1. The project has strong central registries where they matter: tools, toolsets, slash command metadata, providers, plugin hooks, platform adapters.
2. Session persistence is unusually mature for an agent repo. FTS search, compression chains, title resolution, and WAL/retry behavior show real operational experience.
3. Provider support is broad and pragmatic. The code handles OpenAI-compatible providers, native Anthropic, Bedrock, Codex Responses, Copilot ACP, OAuth/device-code flows, custom endpoints, local models, context probing, and fallback chains.
4. The safety posture is serious: approvals, hardline command blocks, smart approval, gateway approval queues, checkpoint rollback, profile isolation, and sensitive-path checks.
5. The dashboard design respects the TUI boundary. It embeds the real terminal UI instead of creating a second divergent chat implementation.
6. The test suite is broad enough to indicate active regression discipline rather than token tests.
7. The plugin system is directionally correct and avoids hardcoding provider-specific features into core when followed.

## Architectural Risks

1. God files are the main maintainability problem. `run_agent.py`, `gateway/run.py`, `cli.py`, `hermes_cli/main.py`, `hermes_cli/web_server.py`, and `tui_gateway/server.py` each carry too many responsibilities.
2. Slash commands have centralized metadata but decentralized behavior. CLI, gateway, TUI, and ACP need shared command services so semantic changes are not reimplemented per transport.
3. The code has significant dynamic import and global mutable state. Examples include plugin discovery side effects, registry generation/cache state, model/tool caches, terminal callback thread-locals, approval contextvars, and process/browser cleanup threads.
4. Config is extremely broad. `hermes_cli/config.py` is comprehensive, but gateway raw YAML loading and CLI `load_cli_config()` paths mean behavior can diverge if new keys are added in the wrong place.
5. Sync/async bridging is necessary but delicate. `model_tools._run_async`, gateway async code, TUI long-handler thread pools, ACP ThreadPoolExecutor, and Modal/Daytona async SDK adapters all increase concurrency risk.
6. The plugin ecosystem expands blast radius. Bundled plugins rely on external services and optional dependencies. Discovery and import timing need strict contract tests.
7. The runtime requires Python 3.11+. System Python 3.9 failed to import modern annotations, which is expected per `pyproject.toml`, but local developer ergonomics depend on using `uv` or a correct venv.
8. The codebase has a high feature density. Adding features without reducing central-file complexity will make regression risk climb faster than test coverage can compensate.

## Recommendations

1. Split `AIAgent` into explicit services:
   - `ProviderRuntime` for client selection and API mode.
   - `PromptAssembler` for system/prefill/memory/context files.
   - `TurnLoop` for model request/retry/fallback.
   - `ToolExecutor` for tool-call normalization/interception/dispatch.
   - `ConversationState` for SessionDB/history/compression coordination.
2. Create a shared slash-command execution layer. Keep `CommandDef` as metadata, but move command behavior into command objects that can render transport-specific output. CLI/gateway/TUI/ACP should call the same command core.
3. Break `gateway/run.py` into runner, command router, session manager, adapter supervisor, approval bridge, background task manager, voice manager, and agent-cache manager.
4. Move dashboard API groups from `hermes_cli/web_server.py` into routers: config, auth/OAuth, sessions, cron, profiles, plugins, chat PTY, analytics.
5. Make config typed. Pydantic/dataclasses for the major config sections would reduce implicit key access and loader drift.
6. Reduce import side effects. Tool/plugin discovery should be explicit at startup boundaries where possible, with tests proving each transport initializes the same registry surface.
7. Add architectural contract tests:
   - Every `CommandDef` has compatible handlers or an explicit unsupported status per transport.
   - Every registered tool belongs to a known/static/dynamic toolset.
   - Plugin imports do not mutate core state before `discover_plugins()`.
   - Gateway/CLI/TUI config loaders agree on shared keys.
8. Keep `scripts/run_tests.sh` mandatory, but add a small "architecture smoke" target that imports major entry points under an isolated `HERMES_HOME` in under a minute.
9. Document ownership boundaries for plugin directories, gateway adapters, and core tools to avoid future hardcoding.

## Boss-Level Readout

This codebase was written by people who understand the operational problems of agent systems: tool loops, provider quirks, context pressure, transport-specific UX, dangerous commands, persistent state, retries, credentials, and platform integrations. It is not clean in the small, but it is competent in the large.

The main criticism is not lack of engineering skill. It is that successful feature growth has concentrated too much policy in a handful of giant orchestrators. The next phase should be architectural extraction, not another round of feature accretion. If the team pays down those central files while preserving the existing registries and tests, Hermes can remain evolvable. If it keeps growing through `run_agent.py`, `gateway/run.py`, and `cli.py`, the cost of safe change will keep rising.

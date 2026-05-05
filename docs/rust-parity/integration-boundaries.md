# Integration Surface Boundaries

This document records the Rust/Python ownership boundary for the non-chat
integration surfaces tracked by `hermes-dwg.4`.

## Cron

Cron remains a Python runtime surface for now. `cron/jobs.py` owns job file
persistence under `HERMES_HOME/cron`, schedule parsing, due-job selection, and
output files. `cron/scheduler.py` owns the tick lock, gateway delivery target
resolution, live adapter delivery, prerun/wake-gate scripts, and `AIAgent`
execution.

Rust owns the contract snapshot in `crates/hermes-integrations`: schedule kinds,
job-management API names, scheduler entry points, delivery modes, known delivery
platforms, and home-target environment variables.

## Batch Runner

`batch_runner.py` remains Python-bound because it uses multiprocessing, dataset
JSONL IO, Python toolset distribution sampling, and `AIAgent` trajectory
conversion. Rust owns the CLI argument list, `BatchRunner.__init__` argument
contract, output/checkpoint file names, and result fields that downstream
training pipelines consume.

## MCP

`mcp_serve.py` remains Python-bound around the dynamic `FastMCP` SDK and the
messaging SessionDB polling bridge. Rust owns the server name, stdio transport
contract, MCP tool list, event types, queue size, and polling interval.

## RL

`rl_cli.py` remains Python-bound because RL workflows depend on Tinker-Atropos
Python environments and async RL tools. Rust owns the CLI argument list,
required environment keys, default toolsets, extended iteration count, terminal
environment overrides, and `AIAgent` keyword contract.

## Plugins

The plugin system intentionally remains dynamic Python code. Rust owns the
stable plugin API boundary: `PluginContext` registration methods, valid hook
names, manifest fields, discovery sources, and dashboard plugin-management
helpers. Plugin-specific API routes stay with each plugin.

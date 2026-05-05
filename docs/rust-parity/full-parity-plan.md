# Full Rust parity and Python removal plan

This document defines the remaining work required before Hermes Agent can
truthfully claim full functional parity with the Python implementation and
remove the Python runtime sources from the shipped product.

The existing `docs/rust-parity/status.yaml` matrix proves contract parity for
selected boundaries. That is necessary, but not sufficient. Full parity means
Rust is the production owner for every supported entry point and the Python
tree can be deleted without breaking a clean install, an existing profile, or
any supported workflow.

## Definition of Done

Full parity is reached only when all of the following are true:

- The `hermes` command can run every supported user workflow through Rust
  without importing in-repo Python.
- Existing `~/.hermes` profiles, config, auth state, session DBs, skills, and
  supported plugin manifests remain readable without a destructive migration.
- CLI, gateway, TUI backend, dashboard, ACP, cron, batch, tools, providers,
  state, skills, and plugin surfaces are Rust-primary in CI and shipped builds.
- A Python-oracle parity suite still exists until removal and compares real
  end-to-end behavior, not only schema snapshots.
- Shadow runtime mode has run across representative mutable and non-mutable
  flows with zero unexplained divergences.
- Rollback is either still available through a released Python fallback, or the
  release notes explicitly mark the no-rollback boundary.
- Python source removal is a separate final commit after every row in the
  `hermes-fpr` epic is `default` or explicitly `deferred` with owner sign-off.

## Workstreams

The `hermes-fpr` epic in `status.yaml` is the source of truth. The rows are:

- Audit the current Rust boundary matrix against every Python entry point.
- Ship a Rust-owned `hermes` binary and runtime selector.
- Make the Rust agent loop production-capable with real provider HTTP,
  streaming, credentials, fallback, interrupts, budgets, and compression.
- Port all tool handlers or declare and gate any non-removable external
  boundary.
- Port gateway runner behavior and production platform adapters.
- Port CLI setup/auth/model/config/update/profile/log/skin surfaces.
- Port TUI gateway, dashboard backend, ACP, cron, batch, MCP, RL, and plugin
  adjacent runtime surfaces.
- Preserve skills, port or defer repo-shipped plugin/provider surfaces, and
  document plugin migration policy. Python plugin ABI compatibility is required
  only for an existing explicit RPC/IPC contract; external Python plugins can
  be converted to Rust on demand.
- Run shadow Python-vs-Rust execution and diffing for representative flows.
- Flip Rust to default and complete the Python removal gate.

The detailed `hermes-fpr.1` audit is in
`docs/rust-parity/entrypoint-audit.md`. It records that the installed
`hermes`, `hermes-agent`, and `hermes-acp` commands are still Python-primary,
and that existing Rust crates prove scoped contracts rather than top-level
runtime ownership.

## Cutover Rules

Each workstream must pass through the existing ladder:

`planned -> in_progress -> ported -> tested -> production_wired -> default`

No workstream may move to `default` in the same commit that introduces new
behavior. Default flips are pure configuration/selection changes so they can be
reverted cleanly.

The final Python deletion is blocked until:

- `scripts/run_tests.sh` passes with Rust as the default runtime.
- `cargo test --workspace` passes.
- All parity fixture/oracle suites pass.
- The installed `hermes` binary runs smoke tests from a clean temp profile and
  from a migrated existing-profile fixture.
- `bd list --status open --status in_progress --status blocked --status deferred`
  shows no unresolved full-parity blockers except explicitly signed-off
  deferrals.

## Non-Goals

- Rewriting behavior without preserving existing config/state compatibility.
- Removing Python tests before the Rust runtime has proven equivalent behavior.
- Treating schema or registry snapshots as a substitute for real workflow
  parity.
- Shipping a Rust facade that shells out to in-repo Python as the final
  architecture.
- Treating external user/pip Python plugin ABI compatibility as a hard cutover
  blocker when no RPC/IPC contract exists.

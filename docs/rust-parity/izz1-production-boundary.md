# hermes-izz.1 — production Rust state boundary

> Replace the cargo-subprocess probe in `hermes_state_rust.py` with a real
> production boundary. **This document is the design; the implementation
> is not landed.**

## Hard constraint: Rust crates do not link to in-repo Python

The Rust state crate (and every future Rust crate in this repo) is a
**standalone reimplementation**. It must not:

- Import or link in-repo Python code.
- Be built as a Python extension whose only purpose is to be loaded by a
  Python interpreter (e.g. PyO3 wheels).
- Embed CPython.

Rust code may use external Rust crates freely. If a Python module in this
repo wraps a third-party library, the Rust side reaches that library
directly via a Rust crate (or, if no such crate exists, via the library's
native interface) — it does not call back into the Python wrapper.

This rules out **Option A — PyO3 + maturin**, which the earlier draft of
this document recommended. It is preserved below in the "Ruled out"
section so future readers don't re-propose it.

## Why this is the gate for the entire `te4` cutover

Today `RustSessionDB` shells out to `cargo run -p hermes-state --bin
hermes_state_probe` for **every operation**. That works for parity tests
but is unusable in production:

| Cost source | Impact |
| --- | --- |
| `cargo run` startup (debug build, fresh process) | ~50–500 ms per op |
| Process fork + JSON serialize/deserialize | ~5–20 ms per op |
| Lost transactionality | each op opens/closes its own SQLite connection |
| Lost connection-level state | WAL checkpoint cadence, busy-handler retries don't carry across ops |
| Cargo workspace lock | concurrent agent processes serialize on the cargo cache |

A typical interactive Hermes session does dozens of state ops per turn.
A 200 ms × 30 ops = 6 s overhead per turn is not survivable.

Until this boundary is replaced, beads `hermes-te4.3` (real entry-point
runs) and `hermes-te4.4` (mandatory CI) cannot land; switching the default
backend would regress every user.

## Recommended: long-running standalone daemon over a Unix domain socket

Spawn one `hermes-state-daemon` (a Rust binary in
`crates/hermes-state/src/bin/`) per `HERMES_HOME`. It owns the SQLite
connection. Python clients connect over a Unix socket and exchange
length-prefixed JSON or `bincode` frames.

This satisfies the standalone-Rust constraint:

- The daemon is a normal Rust binary built via `cargo build`.
- The protocol on the socket is data-only (JSON / bincode). Rust does not
  see Python types and Python does not see Rust types — both sides
  serialize against an OpenAPI-style schema.
- Python clients (`RustSessionDB`) talk to the socket via standard library
  sockets — no FFI, no Python extension, no Rust toolchain at install
  time.

### Why this fits Hermes's deployment shape

- Multiple Hermes processes (gateway + CLI + worktree agents + dashboard)
  can share one daemon per `HERMES_HOME`. The daemon owns the WAL writer,
  so writers are naturally serialized through one process — better than
  the current "every Hermes process opens its own connection and races on
  WAL" model.
- The daemon is auditable as its own service: log file, pid file, restart
  policy, health probe.
- A daemon crash is recoverable: the WAL replay rules SQLite already
  enforces apply on restart.

### Acceptance criteria for the recommended option

1. `crates/hermes-state` adds `src/bin/hermes_state_daemon.rs`. The crate
   keeps zero Python dependencies (no PyO3, no Python.h).
2. `RustSessionDB` autospawns a daemon for the configured `HERMES_HOME`
   if one is not running, then connects to its socket.
3. The daemon uses a stable wire protocol (versioned, length-prefixed
   frames). The protocol schema lives in
   `crates/hermes-state/protocol.md` so other clients can be written.
4. Idle shutdown: the daemon exits after N minutes of inactivity
   (configurable via env var). `RustSessionDB` transparently respawns.
5. Health probe + reconnection loop on `EPIPE` / `ECONNRESET`. Failures
   surface through `RustSessionDB.diagnostics()` (already wired —
   `error_count`, `last_error`).
6. `tests/rust/test_hermes_state_full_parity.py` passes against the
   daemon-backed adapter. Every existing parity assertion continues to
   pass.
7. End-to-end smoke time on a 30-op turn drops below 100 ms total
   overhead vs. the current cargo-subprocess baseline.
8. The daemon binary is shipped via the existing release pipeline (a Rust
   binary alongside other binaries; not a Python wheel).

## Lifecycle and rollback

The factory (`hermes_state_factory.get_session_db`) already handles the
"Rust requested but unavailable" case:

- `HERMES_STATE_BACKEND=rust` (env or arg) → fail fast with a clear
  error.
- `state.backend: rust` (config) → log a warning and fall back to
  Python.

That contract carries over: if the daemon binary is missing, or the
socket connection fails on first attempt, the same fallback logic fires.

## Ruled out: Option A — PyO3 + maturin native extension

The earlier draft of this document recommended building a `pyo3` Python
extension. **This is no longer permitted.** Reasoning:

- The resulting artifact is a Python wheel whose sole purpose is to be
  imported by CPython. Even though no Rust source file imports Python,
  the crate becomes architecturally bound to Python's ABI, GIL, error
  model, and release cadence.
- It conflicts with the project rule that Rust subsystems are standalone
  reimplementations rather than Python-extension shims.
- Wheel-distribution complexity (manylinux/musllinux/macos x86_64+arm64/
  win) is significant operational overhead for what is, in the end, a
  worse architectural choice than a daemon.

## Ruled out: Option C — `cdylib` with `ctypes` from Python

Same problem as Option A in spirit: even though the Rust crate doesn't
import Python, the only consumer of the `cdylib` is Python via FFI, and
the API surface ends up shaped around Python's calling conventions.
Strictly worse than a daemon protocol that any client (Python, Rust,
shell tools, future TUI/CLI) can speak.

## Out of scope for this design

- Schema-version negotiation (covered by `hermes_state.SCHEMA_VERSION`
  and the existing migration logic in the Rust crate).
- Multi-process WAL contention parity (`hermes-izz.2`) — orthogonal: the
  daemon owns the writer, so the Python contention story largely goes
  away, but the existing tests still apply.
- Benchmarks (`hermes-izz.4`) — needs a stable boundary first.

## Suggested next steps

1. File a sub-bead under `hermes-izz.1` for the daemon binary.
2. Define the wire protocol (start with JSON for parity-test ergonomics;
   migrate to `bincode` once the protocol stabilizes).
3. Stand up `src/bin/hermes_state_daemon.rs` with `schema_version` and
   `create_session` only. Get the Python adapter onto the daemon path
   for those two ops.
4. Migrate ops one at a time, gated by the existing parity suite.
5. Replace `cargo run` invocation in `hermes_state_rust.py` once every
   op is daemon-served.

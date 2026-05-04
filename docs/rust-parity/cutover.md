# Rust subsystem cutover and rollback

Companion to [`README.md`](README.md) (the matrix) and
[`status.yaml`](status.yaml) (the source of truth). This document defines
the **gates** every Rust subsystem must clear before becoming default,
and the **rollback path** if a regression appears after cutover.

Tracked by bead `hermes-ni1.4`. Required-section structure is lint-enforced
by `tests/parity/test_cutover_doc.py` so this doc cannot silently lose
its skeleton.

## Status ladder recap

A subsystem moves through `planned → in_progress → ported → tested →
production_wired → default`. This document defines the criteria for each
*forward* transition and the rollback procedure for the *backward*
transitions.

## Parity gates

A Rust subsystem is **parity-passing** when, for every backend-visible
behavior the Python implementation has, the Rust implementation produces
the same result on the same input.

Concrete requirements:

- **Behavior fixtures.** The subsystem's relevant fixtures in
  `tests/parity/` are exercised by both Python and Rust loaders and
  produce identical `ReplayResult` shapes. New fixtures are added as
  new behaviors are ported.
- **Existing test reuse.** When the Python suite already covers the
  behavior (e.g. `tests/test_hermes_state.py`), the Rust adapter is
  exercised against the same suite (see
  `tests/rust/test_hermes_state_full_parity.py` for the pattern).
- **Error parity.** Error types, messages, and recovery paths match —
  not just happy-path returns.
- **Concurrent behavior.** Where the Python implementation has
  documented contention/locking semantics (e.g. SessionDB WAL retry
  with jitter), the Rust path produces the same observable behavior
  under the same load. This is the gate `hermes-izz.2` exists to enforce
  for the state subsystem.

A subsystem reaches the `tested` state in the matrix only when all
parity gates pass in CI on the `rust` job (see `hermes-ni1.3`).

## Performance gates

A Rust subsystem is **performance-passing** when it is no slower than
the Python implementation it replaces and faster on the workloads where
the rewrite was justified.

Concrete requirements:

- **No silent regression.** A benchmark harness exists for the
  subsystem; the harness runs the same workload through both backends
  and reports per-op latency and throughput.
- **Headline workload.** The subsystem's most-used workload (e.g.
  "30 state ops per agent turn" for state) shows total overhead at or
  below the Python baseline. Headline numbers are recorded in the
  subsystem's bead so future regressions are detectable.
- **Tail behavior.** P99 and worst-case behavior are not significantly
  worse than Python, even if mean latency is better. (A 10× P99 spike
  can be worse for users than a flat 2× slowdown.)

For state, this is gated by `hermes-izz.4`. Other subsystems will pick
up the same shape of bead when their port reaches the equivalent stage.

## Data compatibility requirements

A Rust subsystem is **data-compatible** when it reads, writes, and
maintains the same on-disk and over-the-wire formats as the Python
implementation it replaces.

Concrete requirements:

- **Schema versions.** Database subsystems share `SCHEMA_VERSION` with
  the Python implementation and apply the same migrations. The Rust
  state crate already does this — see `crates/hermes-state/src/schema.rs`.
- **No data loss on round-trip.** A session, message, tool call, or
  whatever the subsystem persists, written by Python and read by Rust
  (and vice versa), produces a byte-identical or semantically-identical
  representation.
- **Forward compatibility.** Rust does not write rows or fields that
  Python cannot read. When new fields are added, the Python
  implementation is updated to read them in the same release.
- **Schema changes are coordinated.** A schema-version bump is a
  cross-backend change: Python migration, Rust migration, fixtures,
  and parity tests all land together.

## Rollout flags

A Rust subsystem is rolled out via an explicit flag. The current
patterns:

- **Environment variable.** `HERMES_STATE_BACKEND=rust` is the
  established example. Each subsystem adopting this pattern uses
  `HERMES_<SUBSYSTEM>_BACKEND`.
- **Config key.** Mirrors the env var under the subsystem's config
  section (`state.backend: rust` for state). Config changes do not
  require a restart for subsystems that re-resolve on each instance.
- **Selection precedence.** Explicit arg → env var → config key →
  Python default. This is what `hermes_state_factory.get_session_db`
  implements and is the reference shape for every subsequent
  subsystem.
- **Failure mode.** When the user *explicitly* chooses Rust (arg or
  env), construction failures raise. When the *config* selects Rust
  and Rust is unavailable, the system falls back to Python with a
  logged warning. Config edits never crash production startup.

A subsystem reaches `production_wired` once at least one production
entry point honors the flag. It reaches `default` only after the
subsystem has been `production_wired` for the cutover window
(typically two weeks) without parity or performance regressions.

## Rollback steps

A Rust subsystem can be rolled back by inverting the rollout flag.

Concrete procedure:

1. **Identify the subsystem.** Run the diagnostics surface for the
   suspected subsystem (e.g. `state_backend_diagnostics(db)`) and
   capture the snapshot. The current backend, source of selection,
   any fallback reason, and adapter-level error counts are reported.
2. **Roll back via flag.** Set `HERMES_<SUBSYSTEM>_BACKEND=python`
   for the affected operator's environment, *or* edit the config key
   under that subsystem to `python`. Restart the affected processes.
3. **Verify rollback.** Re-run the diagnostics surface. Confirm
   `backend == "python"` and that subsequent operations succeed.
4. **File a parity-regression bead.** Record the failing input,
   the diagnostics snapshot, and any logs. The bead is required so
   the gap that allowed the regression through CI is closed.
5. **Reproduce in a fixture.** Add a new fixture under
   `tests/parity/` that captures the regression. The Rust port does
   not re-graduate to `tested` until this fixture passes.

A rollback **never** touches on-disk data. The data-compatibility gate
above guarantees Python can read what Rust wrote, so rolling back is
purely a code path change.

## Known deferrals

Subsystems that are explicitly **not** being ported, and why:

- *(Empty at the time of writing.)* When a subsystem is intentionally
  left in Python, its row in `status.yaml` is marked `deferred` with a
  short reason in `notes`, and the subsystem appears here with a
  longer rationale.

The matrix lint will fail if a row is marked `deferred` with no
explanation in this section once a future bead adds one.

## Owner sign-off

Cutover from `production_wired` to `default` requires sign-off from a
subsystem owner. Owners are recorded in `status.yaml` under each row's
`owner` field (currently inherited from the bead, since beads carry
`Owner: ...` metadata). Sign-off is captured by:

- The owner adding a `bd note` to the relevant cutover bead (e.g.
  `hermes-te4.4` for state) confirming the cutover window has elapsed
  with no regressions.
- The matrix row's `status` being bumped from `production_wired` to
  `default` in the same commit that flips the production default.

A subsystem cannot be flipped to `default` in a commit that also lands
new behavior — the cutover commit is purely a flag flip, so a revert
is unambiguous.

## Post-cutover monitoring

After a subsystem is flipped to `default`, the following must be
monitored for at least the **first 14 days**:

- **Diagnostics.** The subsystem's `*_backend_diagnostics()` snapshot
  is queryable from `/status` and is logged at process startup.
  `error_count` and `last_error` should be zero/None for the
  subsystem's instance.
- **CI signal.** The `rust` job in `.github/workflows/tests.yml`
  must remain green. A red rust job after cutover triggers an
  automatic rollback consideration on the relevant bead.
- **User reports.** Any bug filed against the subsystem during the
  monitoring window is triaged for "would this have happened on
  Python?" — if yes, normal bugfix; if no, automatic rollback
  candidate.
- **Performance.** The benchmark harness from the perf gate is
  re-run weekly during the monitoring window and the deltas posted
  in the relevant bead.

After 14 days with all four signals green, the cutover is considered
stable and the monitoring obligation drops to the standard CI signal
plus user reports.

## Per-subsystem cutover plans

The generic gates above apply to every subsystem. The per-subsystem
specifics live in the matrix (`status.yaml` / `README.md`) and in any
bead-specific design docs (e.g.
[`izz1-production-boundary.md`](izz1-production-boundary.md) for the
state backend's production boundary). When a subsystem reaches
`in_progress`, its row's `notes` field links to the bead-specific
cutover doc if the bead-specific gates differ from the generic ones.

# hermes-state

Rust state-store primitives for Hermes Agent.

This crate is the first bounded migration step for `hermes_state.py`. It does
not replace the Python `SessionDB` yet. The current scope is code that can be
ported and tested without changing runtime behavior:

- SQLite schema constants and schema version.
- Session title normalization.
- FTS5 query sanitization.
- CJK codepoint detection/counting used by search fallback logic.
- SQLite schema initialization.
- Core session CRUD.
- Core message append/read behavior, including structured content and tool-call
  counter semantics.
- Atomic message replacement.
- Conversation replay with ancestor-chain loading, duplicate resumed-prompt
  suppression, assistant reasoning fields, and memory-context stripping.
- FTS5 message search with source/exclusion/role filters, context previews,
  tool-field indexing, dangerous-query handling, and CJK trigram/LIKE parity.
- A JSON subprocess probe used by Python tests as the first compatibility
  boundary for exercising the Rust backend.
- An opt-in `RustSessionDB` Python adapter that exposes a focused
  `SessionDB`-shaped compatibility surface for parity tests.
- Session listing, source filtering, counts, exports, message clearing,
  session deletion with child orphaning, and prefix-based session ID
  resolution.
- Title management and lineage helpers.
- Resume/compression tip resolution.
- Prune, empty-ghost cleanup, meta key/value storage, vacuum, and adapter-level
  auto-maintenance behavior.

Next parity step: port rich session listing and compression-tip projection,
then broaden migration/backfill coverage before considering any production
runtime switch.

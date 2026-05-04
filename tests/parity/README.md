# Hermes parity fixtures

Backend-agnostic conversation fixtures used to compare Python and Rust agent
behavior. These fixtures are the **contract**: any backend that calls itself
"Hermes-compatible" must replay these conversations and produce the documented
message sequence, tool calls, tool results, persisted state, and final response.

Tracked by bead `hermes-ni1.2`.

## Layout

```
tests/parity/
  fixtures/
    01_plain_chat.json
    02_single_tool_call.json
    03_multi_turn_tool_use.json
    04_reasoning_field.json
    05_compression_boundary.json
    06_tool_error_recovery.json
  fixture_schema.py        # validation + replay engine
  test_python_parity.py    # validates fixtures + runs reference replay
  test_rust_parity.py      # skeleton; activated when crates/hermes-agent-core lands
```

## Fixture format

Each fixture is a JSON object with three sections:

```json
{
  "id": "single_tool_call",
  "description": "Human-readable summary.",
  "inputs": {
    "user_messages": ["..."],
    "tool_definitions": [ /* OpenAI-style tool schemas */ ],
    "canned_model_responses": [
      {"role": "assistant", "content": "...",
       "tool_calls": [{"id": "call_1", "name": "read_file",
                       "arguments": {"path": "..."}}],
       "reasoning": "optional reasoning text"}
    ],
    "canned_tool_results": {
      "call_1": {"ok": true, "content": "..."}
    }
  },
  "expected": {
    "turn_count": 2,
    "tool_calls_dispatched": [
      {"name": "read_file", "argument_keys": ["path"]}
    ],
    "tool_results_persisted": [
      {"call_id": "call_1", "ok": true}
    ],
    "final_message": {"role": "assistant", "content_contains": "..."},
    "persisted_message_count": 4,
    "persisted_roles": ["user", "assistant", "tool", "assistant"],
    "reasoning_fields_present": false,
    "errors": []
  }
}
```

## Adding a new fixture

1. Create `tests/parity/fixtures/NN_name.json`.
2. Run `.venv/bin/python -m pytest tests/parity/ -v`.
3. The fixture is validated against `fixture_schema.py` and replayed through
   the reference engine. Both Python and (eventually) Rust loaders read the
   same JSON.

## Why this shape

- **Self-contained:** `canned_model_responses` removes provider non-determinism.
- **Backend-agnostic:** the JSON has no Python/Rust types.
- **Concrete:** each `expected` field maps to a checkable behavior in
  `run_agent.py` today (and in the future Rust agent-core).

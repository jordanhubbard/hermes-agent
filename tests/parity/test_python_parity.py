"""Validate parity fixtures and run them through the reference replay engine.

This test does NOT yet drive run_agent.py. It pins the fixture format and
proves each fixture is internally consistent. When hermes-1oa lands, a
companion test will replay the same fixtures through AIAgent with mocked
provider/tool layers.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from tests.parity.fixture_schema import (
    FIXTURE_DIR,
    FixtureError,
    assert_replay_matches_expected,
    iter_fixtures,
    load_fixture,
    replay,
    validate_fixture,
)


def _all_fixture_paths() -> list[Path]:
    return sorted(FIXTURE_DIR.glob("*.json"))


def test_fixture_dir_is_populated() -> None:
    paths = _all_fixture_paths()
    assert paths, f"expected at least one fixture in {FIXTURE_DIR}"


@pytest.mark.parametrize(
    "fixture_path",
    _all_fixture_paths(),
    ids=lambda p: p.stem,
)
def test_fixture_loads_and_validates(fixture_path: Path) -> None:
    load_fixture(fixture_path)


@pytest.mark.parametrize(
    "fixture_path",
    _all_fixture_paths(),
    ids=lambda p: p.stem,
)
def test_reference_replay_matches_expected(fixture_path: Path) -> None:
    fixture = load_fixture(fixture_path)
    result = replay(fixture)
    assert_replay_matches_expected(
        fixture["expected"], result, source=str(fixture_path)
    )


def test_validator_rejects_missing_top_level_key() -> None:
    bad = {
        "id": "x",
        "description": "missing inputs and expected",
    }
    with pytest.raises(FixtureError, match="missing top-level key"):
        validate_fixture(bad)


def test_validator_rejects_dangling_tool_call() -> None:
    bad = {
        "id": "x",
        "description": "tool call with no canned result",
        "inputs": {
            "user_messages": ["hi"],
            "tool_definitions": [],
            "canned_model_responses": [
                {
                    "tool_calls": [
                        {"id": "call_x", "name": "read_file", "arguments": {"p": "1"}}
                    ]
                }
            ],
            "canned_tool_results": {},
        },
        "expected": {
            "turn_count": 1,
            "tool_calls_dispatched": [],
            "tool_results_persisted": [],
            "final_message": None,
            "persisted_message_count": 0,
            "persisted_roles": [],
            "reasoning_fields_present": False,
            "errors": [],
        },
    }
    with pytest.raises(FixtureError, match="canned result"):
        validate_fixture(bad)


def test_validator_rejects_role_count_mismatch() -> None:
    bad = {
        "id": "x",
        "description": "persisted_message_count != len(persisted_roles)",
        "inputs": {
            "user_messages": ["hi"],
            "tool_definitions": [],
            "canned_model_responses": [{"content": "ok"}],
            "canned_tool_results": {},
        },
        "expected": {
            "turn_count": 1,
            "tool_calls_dispatched": [],
            "tool_results_persisted": [],
            "final_message": {"role": "assistant", "content_contains": "ok"},
            "persisted_message_count": 99,
            "persisted_roles": ["user", "assistant"],
            "reasoning_fields_present": False,
            "errors": [],
        },
    }
    with pytest.raises(FixtureError, match="persisted_message_count"):
        validate_fixture(bad)


def test_iter_fixtures_yields_all() -> None:
    paths = list(iter_fixtures())
    assert len(paths) == len(_all_fixture_paths())

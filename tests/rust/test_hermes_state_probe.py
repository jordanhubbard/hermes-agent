"""Python compatibility smoke tests for the Rust hermes-state backend."""

import json
import shutil
import sqlite3
import subprocess
from pathlib import Path

import pytest


REPO_ROOT = Path(__file__).resolve().parents[2]


def _cargo() -> str:
    cargo = shutil.which("cargo")
    if cargo is None:
        pytest.skip("cargo is not available")
    return cargo


def _run_probe(tmp_path: Path, operations: list[dict]) -> list:
    result = subprocess.run(
        [
            _cargo(),
            "run",
            "--quiet",
            "-p",
            "hermes-state",
            "--bin",
            "hermes_state_probe",
            "--",
            "run-json",
            str(tmp_path / "state.db"),
        ],
        cwd=REPO_ROOT,
        input=json.dumps(operations),
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


def test_rust_state_probe_exercises_search_backend_switch(tmp_path):
    """Exercise Rust SessionStore from Python through the subprocess boundary."""

    outputs = _run_probe(
        tmp_path,
        [
            {"op": "schema_version"},
            {"op": "create_session", "id": "s1", "source": "cli", "model": "test-model"},
            {"op": "create_session", "id": "s2", "source": "telegram"},
            {
                "op": "append_message",
                "session_id": "s1",
                "role": "user",
                "content": "Deploy docker containers",
            },
            {
                "op": "append_message",
                "session_id": "s1",
                "role": "assistant",
                "content": "Use docker compose up.",
            },
            {
                "op": "append_message",
                "session_id": "s2",
                "role": "user",
                "content": "今天讨论A2A通信协议的实现",
            },
            {
                "op": "append_message",
                "session_id": "s2",
                "role": "assistant",
                "content": "",
                "tool_name": "UNIQUETOOLNAME",
                "tool_calls": [
                    {
                        "id": "c1",
                        "type": "function",
                        "function": {
                            "name": "UNIQUEFUNCNAME",
                            "arguments": "{\"query\": \"UNIQUESEARCHTOKEN\"}",
                        },
                    }
                ],
            },
            {"op": "search_messages", "query": "docker"},
            {"op": "search_messages", "query": "通信"},
            {"op": "search_messages", "query": "通信", "source_filter": ["cli"]},
            {
                "op": "search_messages",
                "query": "UNIQUETOOLNAME",
                "role_filter": ["assistant"],
            },
            {"op": "get_session", "id": "s1"},
        ],
    )

    assert outputs[0] == 11
    assert outputs[1] == "s1"
    assert outputs[2] == "s2"

    docker_results = outputs[7]
    assert len(docker_results) == 2
    assert {result["source"] for result in docker_results} == {"cli"}
    assert any("docker" in result["snippet"].lower() for result in docker_results)

    cjk_results = outputs[8]
    assert len(cjk_results) == 1
    assert cjk_results[0]["session_id"] == "s2"

    assert outputs[9] == []

    tool_results = outputs[10]
    assert len(tool_results) == 1
    assert tool_results[0]["tool_name"] == "UNIQUETOOLNAME"

    session = outputs[11]
    assert session["model"] == "test-model"
    assert session["message_count"] == 2


def test_rust_state_probe_reconciles_old_schema_and_rebuilds_fts(tmp_path):
    db_path = tmp_path / "state.db"
    conn = sqlite3.connect(db_path)
    conn.executescript(
        """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version (version) VALUES (7);

        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            user_id TEXT,
            model TEXT,
            model_config TEXT,
            system_prompt TEXT,
            parent_session_id TEXT,
            started_at REAL NOT NULL,
            ended_at REAL,
            end_reason TEXT,
            message_count INTEGER DEFAULT 0,
            tool_call_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            api_call_count INTEGER DEFAULT 0
        );

        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT,
            tool_call_id TEXT,
            tool_calls TEXT,
            tool_name TEXT,
            timestamp REAL NOT NULL,
            token_count INTEGER,
            finish_reason TEXT,
            reasoning TEXT,
            reasoning_details TEXT,
            codex_reasoning_items TEXT
        );

        INSERT INTO sessions (id, source, started_at, message_count)
        VALUES ('existing', 'cli', 1000.0, 1);
        INSERT INTO messages (session_id, role, content, tool_name, timestamp)
        VALUES ('existing', 'assistant', '', 'UNIQUETOOLNAME', 1001.0);
        """
    )
    conn.commit()
    conn.close()

    outputs = _run_probe(
        tmp_path,
        [
            {"op": "schema_version"},
            {"op": "get_session", "id": "existing"},
            {"op": "search_messages", "query": "UNIQUETOOLNAME"},
        ],
    )

    assert outputs[0] == 11
    assert outputs[1]["id"] == "existing"
    assert outputs[1]["title"] is None
    assert outputs[1]["cache_read_tokens"] == 0
    assert outputs[1]["api_call_count"] == 0
    assert len(outputs[2]) == 1
    assert outputs[2][0]["session_id"] == "existing"


def test_rust_state_probe_migrates_v2_schema(tmp_path):
    db_path = tmp_path / "state.db"
    conn = sqlite3.connect(db_path)
    conn.executescript(
        """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version (version) VALUES (2);

        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            user_id TEXT,
            model TEXT,
            model_config TEXT,
            system_prompt TEXT,
            parent_session_id TEXT,
            started_at REAL NOT NULL,
            ended_at REAL,
            end_reason TEXT,
            message_count INTEGER DEFAULT 0,
            tool_call_count INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0
        );

        CREATE TABLE messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT,
            tool_call_id TEXT,
            tool_calls TEXT,
            tool_name TEXT,
            timestamp REAL NOT NULL,
            token_count INTEGER,
            finish_reason TEXT
        );

        INSERT INTO sessions (id, source, started_at)
        VALUES ('existing', 'cli', 1000.0);
        """
    )
    conn.commit()
    conn.close()

    outputs = _run_probe(
        tmp_path,
        [
            {"op": "schema_version"},
            {"op": "get_session", "id": "existing"},
            {"op": "set_session_title", "id": "existing", "title": "Migrated Title"},
            {"op": "get_session", "id": "existing"},
        ],
    )

    assert outputs[0] == 11
    assert outputs[1]["title"] is None
    assert outputs[1]["api_call_count"] == 0
    assert outputs[1]["reasoning_tokens"] == 0
    assert outputs[2] is True
    assert outputs[3]["title"] == "Migrated Title"


def test_rust_state_probe_rebuilds_v10_external_content_fts(tmp_path):
    db_path = tmp_path / "state.db"
    conn = sqlite3.connect(db_path)
    conn.executescript(
        """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        INSERT INTO schema_version (version) VALUES (10);

        CREATE TABLE sessions (
            id TEXT PRIMARY KEY,
            source TEXT,
            started_at REAL,
            ended_at REAL,
            title TEXT,
            parent_session_id TEXT,
            message_count INTEGER DEFAULT 0,
            tool_call_count INTEGER DEFAULT 0,
            api_call_count INTEGER DEFAULT 0
        );
        CREATE TABLE messages (
            id INTEGER PRIMARY KEY,
            session_id TEXT NOT NULL,
            timestamp REAL NOT NULL,
            role TEXT NOT NULL,
            content TEXT,
            tool_name TEXT,
            tool_calls TEXT,
            tool_call_id TEXT,
            token_count INTEGER,
            finish_reason TEXT,
            reasoning TEXT,
            reasoning_content TEXT,
            reasoning_details TEXT,
            codex_reasoning_items TEXT,
            codex_message_items TEXT
        );

        CREATE VIRTUAL TABLE messages_fts USING fts5(
            content, content=messages, content_rowid=id
        );
        CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
        END;

        CREATE VIRTUAL TABLE messages_fts_trigram USING fts5(
            content, content=messages, content_rowid=id, tokenize='trigram'
        );
        CREATE TRIGGER messages_fts_trigram_insert AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts_trigram(rowid, content) VALUES (new.id, new.content);
        END;
        """
    )
    conn.execute(
        "INSERT INTO sessions (id, source, started_at) VALUES (?, ?, ?)",
        ("s1", "cli", 1000.0),
    )
    conn.execute(
        "INSERT INTO messages (session_id, timestamp, role, content, tool_name, tool_calls) "
        "VALUES (?, ?, ?, ?, ?, ?)",
        (
            "s1",
            1001.0,
            "assistant",
            "",
            "LEGACYTOOL",
            '{"function":{"name":"web_search","arguments":"{\\"q\\":\\"LEGACYARG\\"}"}}',
        ),
    )
    assert (
        conn.execute(
            "SELECT rowid FROM messages_fts WHERE messages_fts MATCH 'LEGACYTOOL'"
        ).fetchall()
        == []
    )
    conn.commit()
    conn.close()

    outputs = _run_probe(
        tmp_path,
        [
            {"op": "search_messages", "query": "LEGACYTOOL"},
            {"op": "search_messages", "query": "LEGACYARG"},
            {"op": "schema_version"},
        ],
    )

    assert len(outputs[0]) == 1
    assert outputs[0][0]["session_id"] == "s1"
    assert len(outputs[1]) == 1
    assert outputs[1][0]["session_id"] == "s1"
    assert outputs[2] == 11

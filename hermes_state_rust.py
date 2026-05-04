"""Opt-in Python adapter for the Rust hermes-state backend.

This module is a migration aid.  Production Hermes still uses
``hermes_state.SessionDB``; tests can import ``RustSessionDB`` to exercise the
Rust ``SessionStore`` through the JSON probe without changing runtime behavior.
"""

from __future__ import annotations

import json
import os
import shutil
import sqlite3
import subprocess
import threading
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

from hermes_constants import get_hermes_home


DEFAULT_DB_PATH = get_hermes_home() / "state.db"
REPO_ROOT = Path(__file__).resolve().parent


class RustStateBackendError(RuntimeError):
    """Raised when the Rust state probe cannot complete an operation."""


class RustSessionDB:
    """SessionDB-shaped adapter backed by the Rust ``SessionStore``.

    The adapter intentionally shells out to ``hermes_state_probe`` instead of
    importing native code.  That keeps the compatibility boundary explicit
    until the Rust crate has enough parity coverage to justify a real extension
    module or service boundary.
    """

    def __init__(
        self,
        db_path: Path | str | None = None,
        *,
        cargo: str | None = None,
        repo_root: Path | str | None = None,
    ):
        self.db_path = Path(db_path or DEFAULT_DB_PATH)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self.repo_root = Path(repo_root or REPO_ROOT)
        self.cargo = cargo or shutil.which("cargo")
        if not self.cargo:
            raise RustStateBackendError("cargo is required for RustSessionDB")
        self._run_operation({"op": "schema_version"})
        self._lock = threading.RLock()
        self._conn = sqlite3.connect(
            str(self.db_path),
            timeout=30,
            check_same_thread=False,
        )
        self._conn.row_factory = sqlite3.Row
        self._conn.execute("PRAGMA foreign_keys=ON")

    def close(self) -> None:
        """Mirror SessionDB.close(); the subprocess boundary is per operation."""
        if hasattr(self, "_conn") and self._conn is not None:
            self._conn.close()
            self._conn = None

    def _run_operation(self, operation: Dict[str, Any]) -> Any:
        return self._run_operations([operation])[0]

    def _run_operations(self, operations: List[Dict[str, Any]]) -> List[Any]:
        if getattr(self, "_conn", None) is not None:
            self._conn.commit()
        result = subprocess.run(
            [
                self.cargo,
                "run",
                "--quiet",
                "-p",
                "hermes-state",
                "--bin",
                "hermes_state_probe",
                "--",
                "run-json",
                str(self.db_path),
            ],
            cwd=self.repo_root,
            input=json.dumps([_drop_none(operation) for operation in operations]),
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            detail = result.stderr.strip() or result.stdout.strip()
            raise RustStateBackendError(detail or "Rust state probe failed")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise RustStateBackendError(
                f"Rust state probe returned invalid JSON: {result.stdout!r}"
            ) from exc

    def create_session(self, session_id: str, source: str, **kwargs) -> str:
        operation = {
            "op": "create_session",
            "id": session_id,
            "source": source,
            "model": kwargs.get("model"),
            "model_config": kwargs.get("model_config") or None,
            "system_prompt": kwargs.get("system_prompt"),
            "user_id": kwargs.get("user_id"),
            "parent_session_id": kwargs.get("parent_session_id"),
        }
        return self._run_operation(operation)

    def ensure_session(
        self,
        session_id: str,
        source: str = "unknown",
        model: str | None = None,
        **kwargs,
    ) -> str:
        return self.create_session(session_id, source, model=model, **kwargs)

    def get_session(self, session_id: str) -> Optional[Dict[str, Any]]:
        row = self._run_operation({"op": "get_session", "id": session_id})
        return _normalize_session_row(row)

    def resolve_session_id(self, session_id_or_prefix: str) -> Optional[str]:
        return self._run_operation(
            {
                "op": "resolve_session_id",
                "session_id_or_prefix": session_id_or_prefix,
            }
        )

    def resolve_resume_session_id(self, session_id: str) -> str:
        return self._run_operation(
            {"op": "resolve_resume_session_id", "session_id": session_id}
        )

    def get_compression_tip(self, session_id: str) -> str:
        return self._run_operation(
            {"op": "get_compression_tip", "session_id": session_id}
        )

    def end_session(self, session_id: str, end_reason: str) -> None:
        self._run_operation(
            {"op": "end_session", "id": session_id, "end_reason": end_reason}
        )

    def reopen_session(self, session_id: str) -> None:
        self._run_operation({"op": "reopen_session", "id": session_id})

    def update_system_prompt(self, session_id: str, system_prompt: str) -> None:
        self._run_operation(
            {
                "op": "update_system_prompt",
                "id": session_id,
                "system_prompt": system_prompt,
            }
        )

    def set_session_title(self, session_id: str, title: str) -> bool:
        try:
            return self._run_operation(
                {"op": "set_session_title", "id": session_id, "title": title}
            )
        except RustStateBackendError as exc:
            message = str(exc)
            if "already in use" in message or "too long" in message:
                raise ValueError(message) from exc
            raise

    def get_session_title(self, session_id: str) -> Optional[str]:
        return self._run_operation({"op": "get_session_title", "id": session_id})

    def get_session_by_title(self, title: str) -> Optional[Dict[str, Any]]:
        return _normalize_session_row(
            self._run_operation({"op": "get_session_by_title", "title": title})
        )

    def resolve_session_by_title(self, title: str) -> Optional[str]:
        return self._run_operation({"op": "resolve_session_by_title", "title": title})

    def get_next_title_in_lineage(self, base_title: str) -> str:
        return self._run_operation(
            {"op": "get_next_title_in_lineage", "title": base_title}
        )

    def update_token_counts(
        self,
        session_id: str,
        input_tokens: int = 0,
        output_tokens: int = 0,
        model: str = None,
        cache_read_tokens: int = 0,
        cache_write_tokens: int = 0,
        reasoning_tokens: int = 0,
        estimated_cost_usd: Optional[float] = None,
        actual_cost_usd: Optional[float] = None,
        cost_status: Optional[str] = None,
        cost_source: Optional[str] = None,
        pricing_version: Optional[str] = None,
        billing_provider: Optional[str] = None,
        billing_base_url: Optional[str] = None,
        billing_mode: Optional[str] = None,
        api_call_count: int = 0,
        absolute: bool = False,
    ) -> None:
        self._run_operation(
            {
                "op": "update_token_counts",
                "id": session_id,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "model": model,
                "cache_read_tokens": cache_read_tokens,
                "cache_write_tokens": cache_write_tokens,
                "reasoning_tokens": reasoning_tokens,
                "estimated_cost_usd": estimated_cost_usd,
                "actual_cost_usd": actual_cost_usd,
                "cost_status": cost_status,
                "cost_source": cost_source,
                "pricing_version": pricing_version,
                "billing_provider": billing_provider,
                "billing_base_url": billing_base_url,
                "billing_mode": billing_mode,
                "api_call_count": api_call_count,
                "absolute": absolute,
            }
        )

    def append_message(
        self,
        session_id: str,
        role: str,
        content: Any = None,
        tool_name: str = None,
        tool_calls: Any = None,
        tool_call_id: str = None,
        token_count: int = None,
        finish_reason: str = None,
        reasoning: str = None,
        reasoning_content: str = None,
        reasoning_details: Any = None,
        codex_reasoning_items: Any = None,
        codex_message_items: Any = None,
    ) -> int:
        return self._run_operation(
            _message_operation(
                "append_message",
                session_id=session_id,
                role=role,
                content=content,
                tool_name=tool_name,
                tool_calls=tool_calls,
                tool_call_id=tool_call_id,
                token_count=token_count,
                finish_reason=finish_reason,
                reasoning=reasoning,
                reasoning_content=reasoning_content,
                reasoning_details=reasoning_details,
                codex_reasoning_items=codex_reasoning_items,
                codex_message_items=codex_message_items,
            )
        )

    def replace_messages(self, session_id: str, messages: List[Dict[str, Any]]) -> None:
        encoded_messages = [
            _message_operation(
                "message",
                session_id=session_id,
                role=message.get("role", "unknown"),
                content=message.get("content"),
                tool_name=message.get("tool_name"),
                tool_calls=message.get("tool_calls"),
                tool_call_id=message.get("tool_call_id"),
                token_count=message.get("token_count"),
                finish_reason=message.get("finish_reason"),
                reasoning=message.get("reasoning"),
                reasoning_content=message.get("reasoning_content"),
                reasoning_details=message.get("reasoning_details"),
                codex_reasoning_items=message.get("codex_reasoning_items"),
                codex_message_items=message.get("codex_message_items"),
            )
            for message in messages
        ]
        self._run_operation(
            {
                "op": "replace_messages",
                "session_id": session_id,
                "messages": encoded_messages,
            }
        )

    def get_messages(self, session_id: str) -> List[Dict[str, Any]]:
        return self._run_operation({"op": "get_messages", "session_id": session_id})

    def get_messages_as_conversation(
        self, session_id: str, include_ancestors: bool = False
    ) -> List[Dict[str, Any]]:
        return self._run_operation(
            {
                "op": "get_messages_as_conversation",
                "session_id": session_id,
                "include_ancestors": include_ancestors,
            }
        )

    def search_messages(
        self,
        query: str,
        source_filter: List[str] = None,
        exclude_sources: List[str] = None,
        role_filter: List[str] = None,
        limit: int = 20,
        offset: int = 0,
    ) -> List[Dict[str, Any]]:
        return self._run_operation(
            {
                "op": "search_messages",
                "query": query,
                "source_filter": source_filter,
                "exclude_sources": exclude_sources,
                "role_filter": role_filter,
                "limit": limit,
                "offset": offset,
            }
        )

    def search_sessions(
        self,
        source: str = None,
        limit: int = 20,
        offset: int = 0,
    ) -> List[Dict[str, Any]]:
        rows = self._run_operation(
            {
                "op": "search_sessions",
                "source": source,
                "limit": limit,
                "offset": offset,
            }
        )
        return [_normalize_session_row(row) for row in rows]

    def list_sessions_rich(
        self,
        source: str = None,
        exclude_sources: List[str] = None,
        limit: int = 20,
        offset: int = 0,
        include_children: bool = False,
        project_compression_tips: bool = True,
        order_by_last_active: bool = False,
    ) -> List[Dict[str, Any]]:
        rows = self._run_operation(
            {
                "op": "list_sessions_rich",
                "source": source,
                "exclude_sources": exclude_sources,
                "limit": limit,
                "offset": offset,
                "include_children": include_children,
                "project_compression_tips": project_compression_tips,
                "order_by_last_active": order_by_last_active,
            }
        )
        return [_normalize_session_row(row) for row in rows]

    def session_count(self, source: str = None) -> int:
        return self._run_operation({"op": "session_count", "source": source})

    def message_count(self, session_id: str = None) -> int:
        return self._run_operation(
            {"op": "message_count", "session_id": session_id}
        )

    def export_session(self, session_id: str) -> Optional[Dict[str, Any]]:
        row = self._run_operation(
            {"op": "export_session", "session_id": session_id}
        )
        return _normalize_session_row(row)

    def export_all(self, source: str = None) -> List[Dict[str, Any]]:
        rows = self._run_operation({"op": "export_all", "source": source})
        return [_normalize_session_row(row) for row in rows]

    def clear_messages(self, session_id: str) -> None:
        self._run_operation({"op": "clear_messages", "session_id": session_id})

    def delete_session(
        self,
        session_id: str,
        sessions_dir: Optional[Path] = None,
    ) -> bool:
        deleted = self._run_operation(
            {"op": "delete_session", "session_id": session_id}
        )
        if deleted and sessions_dir is not None:
            _remove_session_files(sessions_dir, session_id)
        return deleted

    def prune_sessions(
        self,
        older_than_days: int = 90,
        source: str = None,
        sessions_dir: Optional[Path] = None,
    ) -> int:
        pruned_ids = self._run_operation(
            {
                "op": "prune_sessions",
                "older_than_days": older_than_days,
                "source": source,
            }
        )
        if sessions_dir is not None:
            for session_id in pruned_ids:
                _remove_session_files(sessions_dir, session_id)
        return len(pruned_ids)

    def prune_empty_ghost_sessions(self, sessions_dir: Optional[Path] = None) -> int:
        pruned_ids = self._run_operation({"op": "prune_empty_ghost_sessions"})
        if sessions_dir is not None:
            for session_id in pruned_ids:
                _remove_session_files(sessions_dir, session_id)
        return len(pruned_ids)

    def get_meta(self, key: str) -> Optional[str]:
        return self._run_operation({"op": "get_meta", "key": key})

    def set_meta(self, key: str, value: str) -> None:
        self._run_operation({"op": "set_meta", "key": key, "value": value})

    def vacuum(self) -> None:
        self._run_operation({"op": "vacuum"})

    def _execute_write(self, fn):
        with self._lock:
            result = fn(self._conn)
            self._conn.commit()
            return result

    def maybe_auto_prune_and_vacuum(
        self,
        retention_days: int = 90,
        min_interval_hours: int = 24,
        vacuum: bool = True,
        sessions_dir: Optional[Path] = None,
    ) -> Dict[str, Any]:
        result: Dict[str, Any] = {"skipped": False, "pruned": 0, "vacuumed": False}
        try:
            last_raw = self.get_meta("last_auto_prune")
            now = time.time()
            if last_raw:
                try:
                    last_ts = float(last_raw)
                    if now - last_ts < min_interval_hours * 3600:
                        result["skipped"] = True
                        return result
                except (TypeError, ValueError):
                    pass

            pruned = self.prune_sessions(
                older_than_days=retention_days,
                sessions_dir=sessions_dir,
            )
            result["pruned"] = pruned
            if vacuum and pruned > 0:
                self.vacuum()
                result["vacuumed"] = True
            self.set_meta("last_auto_prune", str(now))
        except Exception as exc:
            result["error"] = str(exc)
        return result


def get_state_db_class(backend: str | None = None):
    """Return the state DB class for ``python`` or ``rust`` backends."""

    selected = (backend or os.getenv("HERMES_STATE_BACKEND") or "python").lower()
    if selected == "rust":
        return RustSessionDB
    if selected == "python":
        from hermes_state import SessionDB

        return SessionDB
    raise ValueError(f"Unsupported HERMES_STATE_BACKEND: {selected}")


def _message_operation(op: str, **kwargs) -> Dict[str, Any]:
    operation = {"op": op}
    operation.update(kwargs)
    return _drop_none(operation)


def _normalize_session_row(row: Optional[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
    if row and isinstance(row.get("model_config"), str):
        try:
            row["model_config"] = json.dumps(json.loads(row["model_config"]))
        except json.JSONDecodeError:
            pass
    return row


def _remove_session_files(sessions_dir: Path, session_id: str) -> None:
    for suffix in (".json", ".jsonl"):
        try:
            (sessions_dir / f"{session_id}{suffix}").unlink(missing_ok=True)
        except OSError:
            pass
    try:
        for path in sessions_dir.glob(f"request_dump_{session_id}_*.json"):
            try:
                path.unlink(missing_ok=True)
            except OSError:
                pass
    except OSError:
        pass


def _drop_none(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: _drop_none(item)
            for key, item in value.items()
            if item is not None
        }
    if isinstance(value, list):
        return [_drop_none(item) for item in value]
    return value

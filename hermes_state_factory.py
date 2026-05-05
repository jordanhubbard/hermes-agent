"""Factory + selection layer for the SessionDB backend.

Production callers should construct state through ``get_session_db()`` rather
than calling ``SessionDB(...)`` directly. The factory:

  * picks a backend (Python is the default; Rust is opt-in)
  * honors ``HERMES_STATE_BACKEND`` env var and a ``state.backend`` config key
  * falls back to Python with a logged warning if the Rust adapter cannot be
    constructed AND the user did not explicitly request Rust
  * exposes ``state_backend_diagnostics()`` for /status surfaces

Tracks beads ``hermes-te4.1`` (factory) and ``hermes-te4.2`` (selection +
diagnostics).
"""

from __future__ import annotations

import logging
import os
import threading
from pathlib import Path
from typing import Any, Dict, Optional

logger = logging.getLogger(__name__)

VALID_BACKENDS = ("python", "rust")
ENV_VAR = "HERMES_STATE_BACKEND"
CONFIG_KEY = "state.backend"

# Diagnostics from the most recent successful selection. Updated under a lock
# so /status surfaces always observe a consistent snapshot.
_LAST_SELECTION_LOCK = threading.Lock()
_LAST_SELECTION: Dict[str, Any] = {
    "backend": None,
    "requested": None,
    "source": None,
    "db_path": None,
    "fallback_reason": None,
}
_LOGGED_ONCE = False


class StateBackendError(RuntimeError):
    """Raised when an explicitly requested backend cannot be constructed."""


def _resolve_request(backend: Optional[str]) -> tuple[str, str]:
    """Return ``(requested, source)`` where source is arg/env/config/default."""
    if backend:
        return backend.lower(), "arg"
    env_value = os.getenv(ENV_VAR)
    if env_value:
        return env_value.lower(), "env"
    cfg_value = _load_config_backend()
    if cfg_value:
        return cfg_value.lower(), "config"
    return "python", "default"


def _load_config_backend() -> Optional[str]:
    try:
        from hermes_cli.config import load_config
    except Exception:
        return None
    try:
        cfg = load_config()
    except Exception as exc:
        logger.debug("state factory: load_config failed (%s)", exc)
        return None
    state_section = cfg.get("state") if isinstance(cfg, dict) else None
    if isinstance(state_section, dict):
        value = state_section.get("backend")
        if isinstance(value, str) and value:
            return value
    return None


def _build_python(db_path: Optional[Path]):
    from hermes_state import SessionDB

    return SessionDB(db_path=db_path) if db_path is not None else SessionDB()


def _build_rust(db_path: Optional[Path]):
    from hermes_state_rust import RustSessionDB

    return RustSessionDB(db_path) if db_path is not None else RustSessionDB()


def get_session_db(
    db_path: Optional[Path] = None,
    *,
    backend: Optional[str] = None,
):
    """Return a SessionDB-shaped instance for the selected backend.

    Selection order: explicit ``backend=`` arg → ``HERMES_STATE_BACKEND`` env
    var → ``state.backend`` config key → ``"python"`` default.

    Raises ``StateBackendError`` only when the user explicitly requested a
    backend (via arg or env var) and that backend cannot be constructed.
    A config-driven Rust selection that fails will fall back to Python with
    a warning, since config edits should not crash production at startup.
    """
    requested, source = _resolve_request(backend)
    if requested not in VALID_BACKENDS:
        raise StateBackendError(
            f"Unsupported state backend {requested!r} (from {source}); "
            f"valid: {VALID_BACKENDS}"
        )

    fallback_reason: Optional[str] = None
    chosen = requested
    instance: Any
    try:
        if requested == "rust":
            instance = _build_rust(db_path)
        else:
            instance = _build_python(db_path)
    except Exception as exc:
        if requested == "rust" and source in ("arg", "env"):
            raise StateBackendError(
                f"Rust state backend explicitly requested via {source} "
                f"but could not be constructed: {exc}"
            ) from exc
        if requested == "rust":
            fallback_reason = f"rust backend unavailable: {exc}"
            logger.warning(
                "state factory: %s; falling back to python (config-driven)",
                fallback_reason,
            )
            chosen = "python"
            instance = _build_python(db_path)
        else:
            raise

    _record_selection(
        chosen=chosen,
        requested=requested,
        source=source,
        db_path=db_path,
        fallback_reason=fallback_reason,
    )
    return instance


def _record_selection(
    *,
    chosen: str,
    requested: str,
    source: str,
    db_path: Optional[Path],
    fallback_reason: Optional[str],
) -> None:
    global _LOGGED_ONCE
    with _LAST_SELECTION_LOCK:
        _LAST_SELECTION.update(
            {
                "backend": chosen,
                "requested": requested,
                "source": source,
                "db_path": str(db_path) if db_path is not None else None,
                "fallback_reason": fallback_reason,
            }
        )
        should_log = not _LOGGED_ONCE
        _LOGGED_ONCE = True
    if should_log:
        if fallback_reason:
            logger.info(
                "state backend: %s (requested=%s source=%s db_path=%s fallback_reason=%s)",
                chosen,
                requested,
                source,
                db_path,
                fallback_reason,
            )
        else:
            logger.info(
                "state backend: %s (source=%s db_path=%s)", chosen, source, db_path
            )


def state_backend_diagnostics(db: Optional[Any] = None) -> Dict[str, Any]:
    """Return a diagnostic snapshot describing the active backend selection.

    Returns ``{"backend": None, ...}`` if ``get_session_db`` has not been
    called yet in this process. If ``db`` is provided and exposes a
    ``diagnostics()`` method (e.g. ``RustSessionDB``), the adapter-level
    diagnostics are merged in under the ``"adapter"`` key.
    """
    with _LAST_SELECTION_LOCK:
        snapshot = dict(_LAST_SELECTION)
    if db is not None and hasattr(db, "diagnostics"):
        try:
            snapshot["adapter"] = db.diagnostics()
        except Exception as exc:
            snapshot["adapter_error"] = str(exc)
    return snapshot


def reset_selection_for_tests() -> None:
    """Clear cached selection state. Tests only — do not call in production."""
    global _LOGGED_ONCE
    with _LAST_SELECTION_LOCK:
        for key in _LAST_SELECTION:
            _LAST_SELECTION[key] = None
        _LOGGED_ONCE = False

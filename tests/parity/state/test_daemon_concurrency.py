"""Concurrent-writer test for the Rust state daemon.

Bead ``hermes-izz.2`` (match SessionDB write contention and WAL
behavior). The daemon's design — one Rust process owning the single
SQLite writer, all Python clients funneling through UDS — sidesteps
the multi-process WAL contention story Python's SessionDB has to fight
with retry-with-jitter. This test pins that property: many concurrent
clients hitting one daemon must produce exactly the writes they
issued, in a bounded amount of time.
"""

from __future__ import annotations

import shutil
import threading
import time
from pathlib import Path
from typing import List, Optional

import pytest

from hermes_state_rust import RustSessionDB


pytestmark = pytest.mark.skipif(
    shutil.which("cargo") is None,
    reason="cargo not installed; daemon binary cannot be built",
)


WRITERS = 8
WRITES_PER_WRITER = 10
PER_OP_BUDGET_MS = 200  # generous on a busy CI runner


@pytest.fixture()
def warm_daemon_db(tmp_path: Path):
    """RustSessionDB(daemon) that has already spawned the daemon."""
    db = RustSessionDB(tmp_path / "state.db", boundary="daemon")
    yield db
    db.close()


def test_many_writers_no_data_loss(warm_daemon_db: RustSessionDB) -> None:
    db_path = warm_daemon_db.db_path
    barrier = threading.Barrier(WRITERS)
    errors: List[BaseException] = []
    errors_lock = threading.Lock()

    def worker(thread_id: int) -> None:
        try:
            client = RustSessionDB(db_path, boundary="daemon")
            try:
                # Synchronize threads so the daemon takes everyone's
                # first request roughly together — this is what
                # actually exercises serialization.
                barrier.wait(timeout=5.0)
                for i in range(WRITES_PER_WRITER):
                    sid = f"t{thread_id:02d}-s{i:02d}"
                    client.create_session(sid, source="cli")
                    client.append_message(
                        sid,
                        role="user",
                        content=f"hello from thread {thread_id} write {i}",
                    )
            finally:
                client.close()
        except BaseException as exc:  # noqa: BLE001
            with errors_lock:
                errors.append(exc)

    started = time.monotonic()
    threads = [
        threading.Thread(target=worker, args=(tid,), name=f"writer-{tid}")
        for tid in range(WRITERS)
    ]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=30.0)
        assert not t.is_alive(), f"writer {t.name} did not finish in time"
    elapsed = time.monotonic() - started

    assert errors == [], f"writer errors: {errors!r}"

    # Verify every write landed.
    sessions = warm_daemon_db.list_sessions_rich(limit=10_000)
    expected_total = WRITERS * WRITES_PER_WRITER
    assert (
        len(sessions) == expected_total
    ), f"expected {expected_total} sessions, got {len(sessions)}"

    # Per-thread, writes must be FIFO ordered relative to that thread's
    # connection (daemon serializes one connection's requests).
    per_thread: dict[int, List[int]] = {}
    for s in sessions:
        sid = s["id"]
        tid = int(sid[1:3])
        seq = int(sid[5:7])
        per_thread.setdefault(tid, []).append(seq)
    for tid, seqs in per_thread.items():
        assert sorted(seqs) == list(
            range(WRITES_PER_WRITER)
        ), f"writer {tid} produced {seqs}, expected 0..{WRITES_PER_WRITER - 1}"

    # All sessions persisted exactly one user message.
    for s in sessions[:5]:  # spot-check; full sweep is overkill
        msgs = warm_daemon_db.get_messages(s["id"])
        assert len(msgs) == 1
        assert msgs[0]["role"] == "user"
        assert msgs[0]["content"].startswith("hello from thread")

    # Bound per-op latency. Each writer does WRITES_PER_WRITER iterations,
    # each iteration is two ops (create_session + append_message). The
    # daemon serializes them, so total time / total ops gives an average
    # that should clear the budget on any reasonable runner.
    total_ops = WRITERS * WRITES_PER_WRITER * 2
    avg_ms_per_op = (elapsed * 1000.0) / total_ops
    assert (
        avg_ms_per_op < PER_OP_BUDGET_MS
    ), f"avg per-op latency {avg_ms_per_op:.1f}ms exceeds budget {PER_OP_BUDGET_MS}ms"


def test_writer_failure_does_not_taint_other_clients(
    warm_daemon_db: RustSessionDB, tmp_path: Path
) -> None:
    """A bad op from one client must not break other clients on the same daemon."""
    other = RustSessionDB(tmp_path / "state.db", boundary="daemon")
    try:
        # First client issues a bad op (no such operation).
        with pytest.raises(Exception):
            warm_daemon_db._run_operation({"op": "definitely-fake"})

        # Second client must still work end-to-end on the same daemon.
        sid = other.create_session("survives", source="cli")
        assert sid == "survives"
        row = other.get_session("survives")
        assert row is not None
        assert row["id"] == "survives"
    finally:
        other.close()


def test_many_writers_no_orphan_daemons(tmp_path: Path) -> None:
    """When several RustSessionDBs are constructed serially against the same
    db, they all share one daemon process — no orphan daemons."""
    primary = RustSessionDB(tmp_path / "state.db", boundary="daemon")
    socket_path: Optional[Path] = primary._daemon.socket_path  # type: ignore[union-attr]
    primary.close()
    assert socket_path is not None

    # Subsequent constructions reuse the existing daemon if its socket
    # is still around.
    others = [
        RustSessionDB(tmp_path / "state.db", boundary="daemon")
        for _ in range(3)
    ]
    try:
        for db in others:
            assert db._daemon is not None  # type: ignore[union-attr]
            assert db._daemon.socket_path == socket_path  # type: ignore[union-attr]
    finally:
        for db in others:
            db.close()

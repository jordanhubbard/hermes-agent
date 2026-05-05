#!/usr/bin/env python3
"""Compare state-backend latency for the three available paths.

Runs an identical sequence of ops (create_session + append_message)
through:

    1. hermes_state.SessionDB             — pure Python (baseline)
    2. RustSessionDB(boundary='subprocess')  — cargo-run-per-op probe
    3. RustSessionDB(boundary='daemon')      — long-running Rust daemon

and prints total time, mean per-op latency, and ops/sec for each.

Tracked by bead hermes-izz.4 (objective perf data before defaulting
to Rust). Run with::

    .venv/bin/python scripts/bench_state.py
    .venv/bin/python scripts/bench_state.py --ops 200
    .venv/bin/python scripts/bench_state.py --skip subprocess  # skip the slow one

Numbers vary heavily by machine. The script prints absolute values
and ratios; the matrix bead carries representative numbers from a
known-good run.
"""

from __future__ import annotations

import argparse
import shutil
import statistics
import sys
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Callable, Iterator, List


def _ensure_repo_on_path() -> None:
    repo_root = Path(__file__).resolve().parents[1]
    if str(repo_root) not in sys.path:
        sys.path.insert(0, str(repo_root))


_ensure_repo_on_path()


from hermes_state import SessionDB  # noqa: E402
from hermes_state_rust import RustSessionDB  # noqa: E402


@contextmanager
def _tempdb() -> Iterator[Path]:
    with tempfile.TemporaryDirectory(prefix="hermes-bench-") as raw:
        yield Path(raw) / "state.db"


def _run_workload(db, ops: int, samples: List[float]) -> None:
    """Issue `ops` create_session + append_message pairs against `db`."""
    for i in range(ops):
        sid = f"bench-{i:05d}"
        t0 = time.perf_counter()
        db.create_session(sid, source="cli")
        db.append_message(sid, role="user", content=f"msg {i}")
        t1 = time.perf_counter()
        samples.append((t1 - t0) * 1000.0)  # ms per (create+append) pair


def _percentile(samples: List[float], pct: float) -> float:
    if not samples:
        return float("nan")
    ordered = sorted(samples)
    k = max(0, min(len(ordered) - 1, int(round((pct / 100.0) * (len(ordered) - 1)))))
    return ordered[k]


def _run_one(label: str, ops: int, build: Callable[[Path], object]) -> dict | None:
    samples: List[float] = []
    with _tempdb() as db_path:
        try:
            db = build(db_path)
        except Exception as exc:  # noqa: BLE001
            print(f"[{label}] skipped: {exc}")
            return None
        try:
            t0 = time.perf_counter()
            _run_workload(db, ops, samples)
            total = time.perf_counter() - t0
        finally:
            try:
                db.close()
            except Exception:
                pass
    return {
        "label": label,
        "ops": ops,
        "total_s": total,
        "mean_ms": statistics.mean(samples) if samples else float("nan"),
        "p50_ms": _percentile(samples, 50.0),
        "p99_ms": _percentile(samples, 99.0),
        "ops_per_s": ops / total if total > 0 else float("nan"),
    }


def _print_table(rows: List[dict]) -> None:
    headers = ["label", "ops", "total_s", "mean_ms", "p50_ms", "p99_ms", "ops_per_s"]
    fmt = {
        "ops": "{:>6}",
        "total_s": "{:>8.3f}",
        "mean_ms": "{:>10.2f}",
        "p50_ms": "{:>9.2f}",
        "p99_ms": "{:>9.2f}",
        "ops_per_s": "{:>10.1f}",
    }
    print()
    print(
        "  ".join(
            [f"{'label':<11}", f"{'ops':>6}", f"{'total_s':>8}",
             f"{'mean_ms':>10}", f"{'p50_ms':>9}", f"{'p99_ms':>9}",
             f"{'ops_per_s':>10}"]
        )
    )
    print("  ".join(["-" * 11, "-" * 6, "-" * 8, "-" * 10, "-" * 9, "-" * 9, "-" * 10]))
    for row in rows:
        cells = [f"{row['label']:<11}"]
        for h in headers[1:]:
            cells.append(fmt[h].format(row[h]))
        print("  ".join(cells))
    print()


def main(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--ops", type=int, default=100, help="ops per backend (default: 100)"
    )
    parser.add_argument(
        "--skip",
        action="append",
        default=[],
        choices=["python", "subprocess", "daemon"],
        help="skip a backend (repeatable)",
    )
    args = parser.parse_args(argv)

    rows: List[dict] = []

    if "python" not in args.skip:
        result = _run_one("python", args.ops, lambda p: SessionDB(db_path=p))
        if result:
            rows.append(result)

    cargo_present = shutil.which("cargo") is not None
    if cargo_present:
        if "subprocess" not in args.skip:
            result = _run_one(
                "subprocess",
                args.ops,
                lambda p: RustSessionDB(p, boundary="subprocess"),
            )
            if result:
                rows.append(result)
        if "daemon" not in args.skip:
            result = _run_one(
                "daemon",
                args.ops,
                lambda p: RustSessionDB(p, boundary="daemon"),
            )
            if result:
                rows.append(result)
    else:
        print("cargo not on PATH; skipping subprocess and daemon backends")

    _print_table(rows)

    if len(rows) >= 2:
        baseline = rows[0]
        print("Ratios vs %s:" % baseline["label"])
        for row in rows[1:]:
            ratio_mean = row["mean_ms"] / baseline["mean_ms"] if baseline["mean_ms"] else float("nan")
            ratio_throughput = (
                row["ops_per_s"] / baseline["ops_per_s"]
                if baseline["ops_per_s"]
                else float("nan")
            )
            faster = "faster" if ratio_mean < 1 else "slower"
            print(
                f"  {row['label']:<11}  mean {ratio_mean:.2f}x  "
                f"({faster}),  throughput {ratio_throughput:.2f}x"
            )

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

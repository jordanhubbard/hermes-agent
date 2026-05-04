#!/usr/bin/env python3
"""Render docs/rust-parity/README.md from docs/rust-parity/status.yaml.

Usage:
    python scripts/rust_parity_status.py            # print summary + table
    python scripts/rust_parity_status.py --write    # also write README.md
    python scripts/rust_parity_status.py --check    # exit 1 if YAML invalid or
                                                    # README.md is out of date
"""

from __future__ import annotations

import argparse
import sys
from collections import Counter
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[1]
STATUS_YAML = REPO_ROOT / "docs" / "rust-parity" / "status.yaml"
RENDERED_MD = REPO_ROOT / "docs" / "rust-parity" / "README.md"

GENERATED_HEADER = (
    "<!-- GENERATED FROM docs/rust-parity/status.yaml — "
    "edit the YAML and run scripts/rust_parity_status.py --write -->"
)


def load_status() -> dict:
    with STATUS_YAML.open() as fh:
        data = yaml.safe_load(fh)
    validate(data)
    return data


def validate(data: dict) -> None:
    statuses = set(data.get("statuses") or [])
    if not statuses:
        raise SystemExit("status.yaml: 'statuses' list is empty")
    seen_beads: set[str] = set()
    for epic in data.get("epics", []):
        for required in ("id", "name", "rows"):
            if required not in epic:
                raise SystemExit(f"status.yaml: epic missing '{required}': {epic}")
        for row in epic["rows"]:
            for required in ("bead", "story", "status"):
                if required not in row:
                    raise SystemExit(
                        f"status.yaml: row in {epic['id']} missing '{required}': {row}"
                    )
            if row["status"] not in statuses:
                raise SystemExit(
                    f"status.yaml: bead {row['bead']} has unknown status "
                    f"'{row['status']}'; valid: {sorted(statuses)}"
                )
            if row["bead"] in seen_beads:
                raise SystemExit(
                    f"status.yaml: bead {row['bead']} appears in more than one row"
                )
            seen_beads.add(row["bead"])


def status_counts(data: dict) -> Counter:
    counts: Counter = Counter()
    for epic in data["epics"]:
        for row in epic["rows"]:
            counts[row["status"]] += 1
    return counts


def render_markdown(data: dict) -> str:
    lines: list[str] = []
    lines.append(GENERATED_HEADER)
    lines.append("")
    lines.append("# Rust parity matrix")
    lines.append("")
    lines.append(
        "Tracks the migration of Hermes subsystems from Python to Rust. "
        "Source of truth: `docs/rust-parity/status.yaml`."
    )
    lines.append("")
    constraints = data.get("constraints") or []
    if constraints:
        lines.append("## Constraints")
        lines.append("")
        for entry in constraints:
            title = entry.get("title", "").strip()
            body = (entry.get("body") or "").strip()
            if title:
                lines.append(f"**{title}.** {body}".rstrip())
            else:
                lines.append(body)
            lines.append("")
    lines.append("## Status ladder")
    lines.append("")
    for status in data["statuses"]:
        lines.append(f"- `{status}`")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    counts = status_counts(data)
    total = sum(counts.values())
    lines.append("| Status | Count | Share |")
    lines.append("| --- | ---: | ---: |")
    for status in data["statuses"]:
        n = counts.get(status, 0)
        pct = (n / total * 100.0) if total else 0.0
        lines.append(f"| `{status}` | {n} | {pct:.0f}% |")
    lines.append(f"| **total** | **{total}** | 100% |")
    lines.append("")
    for epic in data["epics"]:
        lines.append(f"## {epic['id']} — {epic['name']}")
        lines.append("")
        if epic.get("summary"):
            lines.append(epic["summary"])
            lines.append("")
        lines.append("| Bead | Story | Status | Python | Rust target | CI gate |")
        lines.append("| --- | --- | --- | --- | --- | --- |")
        for row in epic["rows"]:
            python_paths = row.get("python_paths") or []
            python_cell = "<br>".join(f"`{p}`" for p in python_paths) or "—"
            rust_cell = f"`{row.get('rust_target', '—')}`"
            ci_cell = f"`{row.get('ci_gate', '—')}`"
            lines.append(
                f"| `{row['bead']}` | {row['story']} | `{row['status']}` "
                f"| {python_cell} | {rust_cell} | {ci_cell} |"
            )
            if row.get("notes"):
                lines.append(f"| | _{row['notes']}_ |  |  |  |  |")
        lines.append("")
    existing = data.get("existing_rust") or []
    if existing:
        lines.append("## Existing Rust footprint (not yet on the cutover ladder)")
        lines.append("")
        lines.append("| Path | State | Description |")
        lines.append("| --- | --- | --- |")
        for entry in existing:
            lines.append(
                f"| `{entry['path']}` | `{entry.get('state', '—')}` | "
                f"{entry.get('description', '')} |"
            )
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write README.md")
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 if YAML invalid or README.md out of date",
    )
    args = parser.parse_args(argv)

    data = load_status()
    rendered = render_markdown(data)
    counts = status_counts(data)

    if args.check:
        existing = RENDERED_MD.read_text() if RENDERED_MD.exists() else ""
        if existing != rendered:
            print(
                "docs/rust-parity/README.md is out of date. "
                "Run: python scripts/rust_parity_status.py --write",
                file=sys.stderr,
            )
            return 1
        print("rust-parity status: ok")
        return 0

    if args.write:
        RENDERED_MD.write_text(rendered)
        print(f"wrote {RENDERED_MD.relative_to(REPO_ROOT)}")

    total = sum(counts.values())
    print(f"rust-parity rows: {total}")
    for status, n in sorted(counts.items()):
        print(f"  {status:>18}: {n}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""Create a sanitized active-plan file from the repository task template."""

from __future__ import annotations

import argparse
import re
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "docs/plans/TEMPLATE.md"
ACTIVE = ROOT / "docs/plans/active"
TASK_ID = re.compile(r"^[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create docs/plans/active/<task-id>-<slug>.md from TEMPLATE.md"
    )
    parser.add_argument("task_id", help="for example HP-CORE-004")
    parser.add_argument("title", help="short human-readable title")
    parser.add_argument(
        "--requirements",
        default="TBD",
        help="comma-separated requirement IDs",
    )
    return parser.parse_args()


def slugify(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "task"


def main() -> None:
    args = parse_args()
    task_id = args.task_id.strip().upper()
    if not TASK_ID.fullmatch(task_id):
        raise SystemExit("task_id must look like HP-CORE-004")

    title = args.title.strip()
    if not title:
        raise SystemExit("title cannot be empty")

    output = ACTIVE / f"{task_id.lower()}-{slugify(title)}.md"
    if output.exists():
        raise SystemExit(f"refusing to overwrite existing plan: {output.relative_to(ROOT)}")

    text = TEMPLATE.read_text(encoding="utf-8")
    text = text.replace("<Task ID>", task_id, 1)
    text = text.replace("<Title>", title, 1)
    text = text.replace("YYYY-MM-DD", date.today().isoformat(), 1)
    text = text.replace("`<requirement IDs>`", f"`{args.requirements.strip()}`", 1)

    ACTIVE.mkdir(parents=True, exist_ok=True)
    output.write_text(text, encoding="utf-8", newline="\n")
    print(output.relative_to(ROOT))


if __name__ == "__main__":
    main()

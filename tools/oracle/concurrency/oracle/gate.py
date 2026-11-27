"""The two-way baseline: every cell that does not hold is recorded, with its issue.

A cell that stops holding and is not recorded fails the run (NEW). A recorded cell that
holds again fails it too (GONE), until the baseline is re-blessed in the same change. A
recorded cell that did not run fails (NOT RUN), and so does an entry without an issue
number (UNNAMED).
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from pathlib import Path

    from oracle.cells import Outcome

COMMENT = [
    "Cells of the concurrency oracle that do not hold, each naming the issue it is tracked by.",
    "Written by `run.sh --bless`; the gate fails on any difference, in either direction.",
]


def load(path: Path) -> dict[str, dict[str, Any]]:
    return json.loads(path.read_text())["cells"]


def compare(outcomes: list[Outcome], cells: dict[str, dict[str, Any]]) -> list[str]:
    """Every difference between the outcomes and the baseline, one line each."""
    problems: list[str] = []
    seen: dict[str, Outcome] = {}
    for o in outcomes:
        if o.key in seen:
            problems.append(f"DUPLICATE {o.key}: reported twice by one run")
        seen[o.key] = o
    for key, o in sorted(seen.items()):
        if not o.holds and key not in cells:
            problems.append(f"NEW {key}: {json.dumps(o.detail)[:300]}")
    for key, entry in sorted(cells.items()):
        if not isinstance(entry.get("issue"), int):
            problems.append(f"UNNAMED {key}: a baseline entry must name its issue")
        if key not in seen:
            problems.append(f"NOT RUN {key}: recorded, but no cell reported it")
        elif seen[key].holds:
            problems.append(f"GONE {key}: holds now; re-bless the baseline in this change")
    return problems


def bless(path: Path, outcomes: list[Outcome], cells: dict[str, dict[str, Any]]) -> list[str]:
    """Rewrites the baseline from the outcomes, keeping each kept entry's issue."""
    blessed = {o.key: cells.get(o.key, {"issue": None}) for o in outcomes if not o.holds}
    path.write_text(
        json.dumps({"_comment": COMMENT, "cells": dict(sorted(blessed.items()))}, indent=2) + "\n"
    )
    return sorted(k for k, v in blessed.items() if not isinstance(v.get("issue"), int))

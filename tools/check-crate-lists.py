#!/usr/bin/env python3
"""No release file keeps its own list of the workspace's crates.

The crates a release publishes, and their order, are derived from the manifests by
tools/publish-order.py. A list written into a workflow, a Dagger module, a script or
the release guide is a second copy, and a second copy goes stale: the publish steps
once missed a crate, and the post-release check had to be edited by hand when
`fraiseql-time` was added.

A list is MIN_LIST (three) or more publishable crate names that stand as list
elements:

  - inline: separated only by list punctuation — blanks, commas, quotes, brackets,
    a shell line continuation (`CRATES="fraiseql-core fraiseql-db"`,
    `[]string{"fraiseql-core", "fraiseql-db"}`, `for c in fraiseql-core fraiseql-db`);
  - stacked: on consecutive lines that each hold just a crate name, or are a markdown
    list item opening with one (`1. fraiseql-error (no dependencies)`).

The rule is on the names, wherever they stand, not on a variable called CRATES. Two
names are not a list: a pair is the two binaries, or a deliberate two-crate pair, never
a copy of the publish set. The crate set itself comes from the manifests
(`publish-order.py`'s tomllib reader), so the check cannot go stale either.

A list that is a deliberate subset (the crates a semver check covers, say) says so where
it stands: a comment on its first line, or on the line above, holding
`crate-subset: <reason>` with a reason of at least three words. In markdown the comment
is `<!-- crate-subset: … -->`.

Usage: check-crate-lists.py
Overrides, for testing: CRATE_LISTS_ROOT=<workspace root>.
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
from pathlib import Path

MIN_LIST = 3
MARKER = re.compile(r"crate-subset:\s*(.*?)\s*(?:-->)?\s*$")
ROOT = Path(os.environ.get("CRATE_LISTS_ROOT") or Path(__file__).resolve().parent.parent)
SCOPE = (
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
    ".dagger/*.go",
    "tools/*.sh",
    "tools/lib/*.sh",
    "releasing.md",
)
SEPARATOR = r"""[\s,"'`\[\]{}()\\]*"""
ITEM = re.compile(r"^\s*(?:[-*+]|\d+[.)])\s+`?([A-Za-z0-9_-]+)`?(?=[\s,:(]|$)")
BARE = re.compile(r"""^[\s"'`,\[\]{}\\]*([A-Za-z0-9_-]+)[\s"'`,\[\]{}\\]*$""")

_spec = importlib.util.spec_from_file_location(
    "_fraiseql_publish_order", Path(__file__).resolve().parent / "publish-order.py"
)
publish_order = importlib.util.module_from_spec(_spec)
sys.modules["_fraiseql_publish_order"] = publish_order
_spec.loader.exec_module(publish_order)


def inline_pattern(crates: list[str]) -> re.Pattern[str]:
    name = "|".join(re.escape(c) for c in sorted(crates, key=len, reverse=True))
    one = rf"(?<![\w/.-])(?:{name})(?![\w/.-])"
    return re.compile(rf"{one}(?:{SEPARATOR}{one})+")


def logical_lines(text: str) -> list[tuple[int, str]]:
    """Lines with shell continuations (`\\` at the end) joined, each with its first line number."""
    out: list[tuple[int, str]] = []
    pending, start = "", 0
    for number, line in enumerate(text.splitlines(), 1):
        if not pending:
            start = number
        if line.rstrip().endswith("\\"):
            pending += line.rstrip()[:-1] + " "
            continue
        out.append((start, pending + line))
        pending = ""
    if pending:
        out.append((start, pending))
    return out


def stacked_name(line: str, crates: set[str]) -> str | None:
    for pattern in (ITEM, BARE):
        m = pattern.match(line)
        if m and m.group(1) in crates:
            return m.group(1)
    return None


def find_lists(text: str, crates: list[str]) -> list[tuple[int, list[str]]]:
    """Every list of MIN_LIST or more crate names in `text`: (first line, names)."""
    found: list[tuple[int, list[str]]] = []
    inline = inline_pattern(crates)
    names = re.compile("|".join(re.escape(c) for c in sorted(crates, key=len, reverse=True)))
    for number, line in logical_lines(text):
        for m in inline.finditer(line):
            found.append((number, names.findall(m.group(0))))
    run: list[str] = []
    run_start = 0
    lines = text.splitlines()
    for number, line in enumerate([*lines, ""], 1):
        name = stacked_name(line, set(crates)) if number <= len(lines) else None
        if name:
            if not run:
                run_start = number
            run.append(name)
            continue
        if run:
            found.append((run_start, run))
        run = []
    return [(n, listed) for n, listed in found if len(set(listed)) >= MIN_LIST]


def marker_reason(lines: list[str], number: int) -> str | None:
    """The `crate-subset:` reason on line `number` (1-based) or the line above, if any."""
    for line in lines[max(number - 2, 0) : number]:
        m = MARKER.search(line)
        if m:
            return m.group(1)
    return None


def main() -> int:
    crates = publish_order.publishable_workspace_crates(ROOT)
    if len(crates) < 2:
        print(f"crate lists: FAIL — {ROOT} publishes {len(crates)} crate(s); nothing to check")
        return 1
    files = sorted({p for pattern in SCOPE for p in ROOT.glob(pattern) if p.is_file()})
    if not files:
        print("crate lists: FAIL — no release file found to scan")
        return 1
    hits = []
    for path in files:
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        for number, names in find_lists(text, crates):
            where = f"{path.relative_to(ROOT)}:{number}"
            reason = marker_reason(lines, number)
            if reason is None:
                shown = ", ".join(names[:4]) + (", …" if len(names) > 4 else "")
                hits.append(f"{where}: {len(names)} crates listed ({shown})")
            elif len(re.findall(r"[A-Za-z]{2,}", reason)) < 3:
                hits.append(f"{where}: `crate-subset: {reason}` gives no reason in words")
    if hits:
        print("\n".join(f"crate lists: FAIL — {h}" for h in hits))
        print(
            "crate lists: read the crates from `python3 tools/publish-order.py` "
            "(the order) or `--set` (the set, no toolchain) instead of listing them; a "
            "deliberate subset says why with a `crate-subset: <reason>` comment"
        )
        return 1
    print(
        f"crate lists: ok — {len(files)} release file(s) scanned; none lists the "
        f"{len(crates)} publishable crates by hand"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

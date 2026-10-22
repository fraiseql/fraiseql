#!/usr/bin/env python3
"""The documentation's Rust examples are compiled: every doc with one is a doctest.

A fenced ```rust block in a document claims to compile. `crates/fraiseql/src/lib.rs`
includes each such document into a `#[cfg(doctest)]` module, so the test leg's
`cargo test --doc --all-features` compiles every plain ```rust block in it (a block
that only illustrates is fenced ```rust,ignore or ```text). This gate keeps that list
complete, and keeps the rule unambiguous:

1. Every markdown file in README.md and docs/ (history and ADRs excepted) that has a
   ```rust block is listed in the doctest module, and every listed file exists and
   still has one.
2. No fence there is bare. rustdoc compiles an unlabeled fence as Rust, so a bare
   fence in a listed file is a compile claim nobody made, and one in an unlisted file
   is ambiguous. Diagrams and output are ```text.

Overrides, for testing: DOCS_RUST_ROOT=<dir>.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

HISTORY = ("docs/history/", "docs/security/audits/", "docs/migration/", "docs/adr/")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")
INCLUDE = re.compile(r'include_str!\("\.\./\.\./\.\./([^"]+)"\)')


def fences(text: str):
    """(line, info string) of each opening fence."""
    inside = None
    for n, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if not m:
            continue
        if inside is None:
            inside = m.group(1)
            yield n, m.group(2).strip()
        elif (
            m.group(1)[0] == inside[0]
            and len(m.group(1)) >= len(inside)
            and not m.group(2).strip()
        ):
            # CommonMark: a fence closes on the same character, at least as long.
            inside = None


def main() -> int:
    root = Path(os.environ.get("DOCS_RUST_ROOT") or Path(__file__).resolve().parent.parent)
    lib = root / "crates/fraiseql/src/lib.rs"
    if not lib.is_file():
        print(f"docs rust examples: {lib} not found", file=sys.stderr)
        return 1
    listed = set(INCLUDE.findall(lib.read_text(encoding="utf-8")))

    docs = [root / "README.md", *sorted((root / "docs").rglob("*.md"))]
    with_rust, failures = set(), []
    for path in docs:
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if rel.startswith(HISTORY):
            continue
        for line, info in fences(path.read_text(encoding="utf-8")):
            if not info:
                failures.append(f"{rel}:{line}: bare fence; name its language (```text for diagrams)")
            elif info.split(",")[0].strip() == "rust":
                with_rust.add(rel)

    if not with_rust:
        failures.append("no document has a ```rust block, so there is nothing to check")
    failures += [
        f"{rel}: has a ```rust block but is not included in {lib.relative_to(root)}'s doctest module"
        for rel in sorted(with_rust - listed)
    ]
    failures += [
        f"{rel}: included in the doctest module but has no ```rust block (or is gone)"
        for rel in sorted(listed - with_rust)
    ]
    if failures:
        print("docs rust examples: FAIL", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"docs rust examples: ok ({len(with_rust)} documents compiled as doctests, no bare fences)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

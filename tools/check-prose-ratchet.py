#!/usr/bin/env python3
"""The prose ratchet: issue numbers in code comments may only go down.

An issue number in a code comment (`// see #1234`) explains a line by pointing at
history. It goes stale the day the issue closes, it says nothing a reader of the code
can check, and it is the commit message's job. The tree carries thousands of them; the
diet is a later milestone. This gate only stops the count from rising, per crate, so
new code explains itself in the comment and cites the issue in the commit.

What counts
-----------
`#NNN` or `#NNNN` (an issue reference) inside a plain comment, in non-test library
and binary code under `crates/*/src/`:

* plain comments are `//` and `/* */`, and `////` (four slashes is not a doc comment);
* doc comments (`///`, `//!`, `/** */`, `/*! */`) are not counted: they are API
  contract, rendered by rustdoc, and a reference there documents behaviour;
* string and char literals, raw strings included, are not comments;
* test code is not counted: files under a `tests/` directory, `tests.rs`,
  `*_tests.rs`, `*_test.rs`, and the body of an inline module carrying
  `#[cfg(test)]`.

The baseline
------------
`tools/prose-ratchet-baseline.txt` holds one `<crate> <count>` line per crate. A crate
above its count fails: move the reference into the commit message. A crate below its
count also fails, with the line to write: the ratchet only holds if each drop is
locked in. A crate missing from the baseline, or a baseline line for a crate that no
longer exists, fails.

Overrides, for testing: PROSE_RATCHET_ROOT=<dir> (the repository root),
PROSE_RATCHET_BASELINE=<file>.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ISSUE = re.compile(r"(?<![\w#])#\d{3,4}(?!\d)")


def lex(src: str) -> tuple[list[tuple[int, int, bool]], list[tuple[int, int]]]:
    """Every comment in `src` as (start, end, is_doc), and every string or char literal."""
    spans: list[tuple[int, int, bool]] = []
    literals: list[tuple[int, int]] = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            end = src.find("\n", i)
            end = n if end == -1 else end
            doc = src.startswith("///", i) and not src.startswith("////", i)
            doc = doc or src.startswith("//!", i)
            spans.append((i, end, doc))
            i = end
        elif src.startswith("/*", i):
            doc = (src.startswith("/**", i) and not src.startswith("/**/", i)) or src.startswith(
                "/*!", i
            )
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            spans.append((i, j, doc))
            i = j
        elif c == "r" and re.match(r'r#*"', src[i:]) and _raw_string_start(src, i):
            hashes = len(re.match(r"r(#*)", src[i:]).group(1))
            close = '"' + "#" * hashes
            end = src.find(close, i + 2 + hashes)
            end = n if end == -1 else end + len(close)
            literals.append((i, end))
            i = end
        elif c == '"':
            end = _skip_string(src, i)
            literals.append((i, end))
            i = end
        elif c == "'":
            # A char literal ('x', '\n', '\u{1F}') or a lifetime ('a): skip only the former.
            m = re.match(r"'(\\u\{[0-9a-fA-F]+\}|\\.|[^\\'\n])'", src[i:])
            if m:
                literals.append((i, i + len(m.group(0))))
            i = i + len(m.group(0)) if m else i + 1
        else:
            i += 1
    return spans, literals


def _ident_before(src: str, i: int) -> bool:
    return i > 0 and (src[i - 1].isalnum() or src[i - 1] == "_")


def _raw_string_start(src: str, i: int) -> bool:
    """`r` at `i` opens a raw string (`r"`, `r#"`, `br#"`), not the end of an identifier."""
    if i > 0 and src[i - 1] == "b":
        return not _ident_before(src, i - 1)
    return not _ident_before(src, i)


def _skip_string(src: str, i: int) -> int:
    """Index just past the string literal whose opening quote is at `i`."""
    j = i + 1
    while j < len(src):
        if src[j] == "\\":
            j += 2
        elif src[j] == '"':
            return j + 1
        else:
            j += 1
    return j


CFG_TEST_MOD = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{")


def test_module_spans(
    src: str, comments: list[tuple[int, int, bool]], literals: list[tuple[int, int]]
) -> list[tuple[int, int]]:
    """The bodies of inline `#[cfg(test)] mod x { … }` blocks, braces matched in code only."""
    in_comment = [False] * (len(src) + 1)
    for start, end in [(a, b) for a, b, _ in comments] + literals:
        for k in range(start, end):
            in_comment[k] = True
    spans = []
    for m in CFG_TEST_MOD.finditer(src):
        if in_comment[m.start()]:
            continue
        depth, j = 1, m.end()
        while j < len(src) and depth:
            if not in_comment[j]:
                if src[j] == "{":
                    depth += 1
                elif src[j] == "}":
                    depth -= 1
            j += 1
        spans.append((m.start(), j))
    return spans


def is_test_file(rel: Path) -> bool:
    name = rel.name
    return (
        "tests" in rel.parts[:-1]
        or name == "tests.rs"
        or name.endswith(("_tests.rs", "_test.rs"))
    )


def count_file(src: str) -> int:
    comments, literals = lex(src)
    tests = test_module_spans(src, comments, literals)
    total = 0
    for start, end, doc in comments:
        if doc or any(a <= start < b for a, b in tests):
            continue
        total += len(ISSUE.findall(src, start, end))
    return total


def count_crates(root: Path) -> dict[str, int]:
    counts: dict[str, int] = {}
    for crate in sorted((root / "crates").iterdir()) if (root / "crates").is_dir() else []:
        src = crate / "src"
        if not src.is_dir():
            continue
        total = 0
        for f in sorted(src.rglob("*.rs")):
            if is_test_file(f.relative_to(src)):
                continue
            total += count_file(f.read_text(encoding="utf-8", errors="replace"))
        counts[crate.name] = total
    return counts


def main() -> int:
    root = Path(os.environ.get("PROSE_RATCHET_ROOT") or Path(__file__).resolve().parent.parent)
    baseline_path = Path(
        os.environ.get("PROSE_RATCHET_BASELINE") or root / "tools/prose-ratchet-baseline.txt"
    )
    counts = count_crates(root)
    if not counts:
        print(f"prose ratchet: no crates/*/src under {root}; nothing to check", file=sys.stderr)
        return 1
    if "--print" in sys.argv:
        for name, n in counts.items():
            print(f"{name} {n}")
        return 0
    if not baseline_path.is_file():
        print(f"prose ratchet: {baseline_path} not found", file=sys.stderr)
        return 1
    baseline: dict[str, int] = {}
    for line in baseline_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, n = line.split()
        baseline[name] = int(n)

    failures = []
    for name, n in counts.items():
        if name not in baseline:
            failures.append(f"{name}: not in the baseline; add `{name} {n}`")
        elif n > baseline[name]:
            failures.append(
                f"{name}: {n} issue references in code comments, baseline {baseline[name]}. "
                "Say in the comment what the code does and why; cite the issue in the commit."
            )
        elif n < baseline[name]:
            failures.append(f"{name}: down to {n} from {baseline[name]}; lock it in: `{name} {n}`")
    failures += [f"{name}: in the baseline but not a crate" for name in baseline if name not in counts]

    if failures:
        print("prose ratchet: FAIL", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"prose ratchet: ok ({sum(counts.values())} references across {len(counts)} crates, none added)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

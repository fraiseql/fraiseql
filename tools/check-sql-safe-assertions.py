#!/usr/bin/env python3
"""Every `sqlx::AssertSqlSafe` states why the SQL it wraps is safe.

WHY THIS GATE EXISTS
--------------------
sqlx's query functions take SQL text only as a `&'static str` or wrapped in
`AssertSqlSafe`: text built at runtime does not compile until someone asserts it holds
no value that should have been bound. The assertion is only worth what was checked
before writing it, and a bare `AssertSqlSafe(format!(...))` says nothing about that. So
each one carries a `// Reason:` naming what the text is built from (a constant, a name
passed through `quote_ident`, the operator's own configured query, ...), where a
reviewer reads it beside the call.

RULES
-----
1. Every code occurrence of `AssertSqlSafe(` has a `// Reason: <text>` comment, either
   at the end of its own line or in the block of comment lines directly above the line
   its statement starts on. A statement that continues onto the call's line (the line
   before ends with `(` or `=`) starts where it starts; a blank line or a line of code
   between the comment and the statement breaks the block.
2. `AssertSqlSafe` appears in code only as that call, spelled by path. A `use` of it, or
   an alias, would let a call elsewhere read as something else.
3. Scanning the tree finds at least one call. Finding none means the pattern this gate
   reads has moved, not that the tree is clean.

POSITIONAL, NOT TEXTUAL
-----------------------
Only code is matched: comments, doc comments and string literals may name
`AssertSqlSafe` freely.

WHAT THIS GATE DOES NOT REACH
-----------------------------
`sqlx::QueryBuilder` asserts its own text when it builds the query, so text pushed into
one with `push` is not seen here.

`SQL_SAFE_ROOT` points the gate at another directory (every `.rs` under it), for its
tests; rule 3 then does not apply.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

NAME = "AssertSqlSafe"
REASON = re.compile(r"//\s*Reason:\s*\S")


def code_mask(text: str) -> str:
    """`text` with every comment and string literal blanked, newlines kept."""
    out = []
    i, n = 0, len(text)

    def blank(s: str) -> str:
        return "".join(c if c == "\n" else " " for c in s)

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(blank(text[i:j]))
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            out.append(blank(text[i:j]))
            i = j
        elif m := re.match(r'b?r(#*)"', text[i:]):
            if i and (text[i - 1].isalnum() or text[i - 1] == "_"):
                out.append(c)
                i += 1
                continue
            close = '"' + m.group(1)
            j = text.find(close, i + len(m.group(0)))
            j = n if j < 0 else j + len(close)
            out.append(blank(text[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j + 1, n)
            out.append(blank(text[i:j]))
            i = j
        elif c == "'" and (m := re.match(r"'(\\.[^']*|[^\\'])'", text[i:])):
            out.append(blank(m.group(0)))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def check_file(path: Path, display: str) -> tuple[list[str], int]:
    text = path.read_text(encoding="utf-8", errors="replace")
    if NAME not in text:
        return [], 0
    lines = text.split("\n")
    code = code_mask(text).split("\n")
    problems: list[str] = []
    calls = 0
    for index, line in enumerate(code):
        for m in re.finditer(rf"\b{NAME}\b(\s*\()?", line):
            where = f"{display}:{index + 1}"
            if not m.group(1):
                problems.append(f"{where}: `{NAME}` outside a call by path (rule 2)")
                continue
            calls += 1
            if REASON.search(lines[index][m.end():]):
                continue
            start = index
            while start > 0 and code[start - 1].rstrip().endswith(("(", "=")):
                start -= 1
            above = start - 1
            found = False
            while above >= 0 and lines[above].strip().startswith("//"):
                if REASON.search(lines[above]):
                    found = True
                    break
                above -= 1
            if not found:
                problems.append(
                    f"{where}: `{NAME}(` with no `// Reason:` beside it (rule 1)"
                )
    return problems, calls


def tracked_sources(root: Path) -> list[Path]:
    listed = subprocess.run(
        # Untracked files too: a new file is not in the index until it is staged.
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--"]
        + ["crates", "tools", "examples"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [root / p for p in listed.split("\0") if p.endswith(".rs")]


def main() -> int:
    fixture_root = os.environ.get("SQL_SAFE_ROOT")
    if fixture_root:
        root = Path(fixture_root)
        files = sorted(root.rglob("*.rs"))
    else:
        root = Path(__file__).resolve().parent.parent
        files = tracked_sources(root)
    problems: list[str] = []
    calls = 0
    for path in files:
        found, n = check_file(path, str(path.relative_to(root)))
        problems += found
        calls += n
    if not fixture_root and calls == 0:
        problems.append(f"no `{NAME}(` call found under crates/, tools/ or examples/ (rule 3)")
    if problems:
        print("sql-safe assertions: FAILED", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"sql-safe assertions: ok ({calls} calls, each with its reason)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

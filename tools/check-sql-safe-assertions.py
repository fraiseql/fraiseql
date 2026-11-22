#!/usr/bin/env python3
"""Every SQL text sqlx cannot check states why it is safe.

WHY THIS GATE EXISTS
--------------------
sqlx's query functions take SQL text only as a `&'static str` or wrapped in
`AssertSqlSafe`: text built at runtime does not compile until someone asserts it holds
no value that should have been bound. The assertion is only worth what was checked
before writing it, and a bare `AssertSqlSafe(format!(...))` says nothing about that. So
each one carries a `// Reason:` naming what the text is built from (a constant, a name
passed through `quote_ident`, the operator's own configured query, ...), where a
reviewer reads it beside the call.

`sqlx::QueryBuilder` asserts its own text when it builds the query, so the text given to
it is held to the same rule: a string literal passes as it is, and anything else
carries a `// Reason:`.

RULES
-----
1. Every code occurrence of `AssertSqlSafe(` has a `// Reason: <text>` comment, either
   at the end of its own line or in the block of comment lines directly above the line
   its statement starts on. A statement that continues onto the call's line (the line
   before ends with `(` or `=`) starts where it starts; a blank line or a line of code
   between the comment and the statement breaks the block.
2. `AssertSqlSafe` appears in code only as that call, spelled by path. A `use` of it, or
   an alias, would let a call elsewhere read as something else.
3. Every call that hands a `sqlx::QueryBuilder` text, `QueryBuilder::new(..)`,
   `QueryBuilder::with_arguments(..)`, and `push(..)` or `push_unseparated(..)` on a
   builder or on what its `separated(..)` returns, has a string literal for that text
   or a `// Reason:` placed as in rule 1. A builder is a name bound with a builder
   type (`let qb: sqlx::QueryBuilder<..>`, a parameter `query: &mut sqlx::QueryBuilder<..>`),
   bound to a constructor call, or bound to a builder's `separated(..)`; a `push` is on
   a builder when the method chain it ends leads back to one of those names.
4. `sqlx::QueryBuilder` and `sqlx::query_builder::Separated` appear in code only where
   rule 3 can follow them: as the type of a named binding, parameter or field, or
   called as a constructor. A `use`, an alias, a return type or a type argument would
   let a builder reach a name this gate never sees.
5. Scanning the tree finds at least one `AssertSqlSafe(` call and at least one call
   handing a builder text. Finding none means the pattern this gate reads has moved,
   not that the tree is clean.

POSITIONAL, NOT TEXTUAL
-----------------------
Only code is matched: comments, doc comments and string literals may name
`AssertSqlSafe` or `QueryBuilder` freely.

`SQL_SAFE_ROOT` points the gate at another directory (every `.rs` under it), for its
tests; rule 5 then does not apply.
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


def has_reason(lines: list[str], code: list[str], index: int, after: int) -> bool:
    """A `// Reason:` at the end of line `index` (from column `after`), or in the comment
    block directly above the line the statement holding that line starts on."""
    if REASON.search(lines[index][after:]):
        return True
    start = index
    while start > 0 and code[start - 1].rstrip().endswith(("(", "=")):
        start -= 1
    above = start - 1
    while above >= 0 and lines[above].strip().startswith("//"):
        if REASON.search(lines[above]):
            return True
        above -= 1
    return False


BUILDER = r"sqlx::(?:query_builder::)?(?:QueryBuilder|Separated)\b"
TURBOFISH = r"(?:\s*::\s*<[^;{}()]*?>)?"
CONSTRUCTOR = re.compile(rf"{BUILDER}{TURBOFISH}\s*::\s*(new|with_arguments)\s*\(")
TYPED_NAME = re.compile(rf"\b(\w+)\s*:\s*(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?{BUILDER}")
BOUND_TO_CONSTRUCTOR = re.compile(
    rf"\blet\s+(?:mut\s+)?(\w+)\s*(?::[^=;]*)?=\s*{BUILDER}{TURBOFISH}\s*::\s*(?:new|with_arguments)\s*\("
)
BOUND_TO_SEPARATED = re.compile(
    r"\blet\s+(?:mut\s+)?(\w+)\s*=\s*([^;]*?)\.\s*separated\s*\("
)
TEXT_METHOD = re.compile(r"\.\s*(push|push_unseparated)\s*\(")
IDENT = re.compile(r"[A-Za-z_]\w*$")


def closing(code: str, open_at: int) -> int:
    """Index of the `)` matching the `(` at `open_at` (strings are already blanked)."""
    depth = 0
    for i in range(open_at, len(code)):
        if code[i] in "([{":
            depth += 1
        elif code[i] in ")]}":
            depth -= 1
            if depth == 0:
                return i
    return len(code)


def opening(code: str, close_at: int) -> int:
    """Index of the `(` matching the `)` at `close_at`."""
    depth = 0
    for i in range(close_at, -1, -1):
        if code[i] in ")]}":
            depth += 1
        elif code[i] in "([{":
            depth -= 1
            if depth == 0:
                return i
    return -1


def receiver(code: str, dot: int) -> str | None:
    """The name a method chain ending at the `.` at `dot` starts from: in
    `query.push(a).push_bind(b).push(c)`, `query` for every `push`."""
    i = dot - 1
    while True:
        while i >= 0 and code[i].isspace():
            i -= 1
        if i < 0:
            return None
        if code[i] == ")":
            # A call earlier in the chain: step over its arguments and its method name.
            i = opening(code, i) - 1
            while i >= 0 and code[i].isspace():
                i -= 1
            m = IDENT.search(code, 0, i + 1)
            if not m:
                return None
            i = m.start() - 1
            while i >= 0 and code[i].isspace():
                i -= 1
            if i < 0 or code[i] != ".":
                return None
            i -= 1
            continue
        m = IDENT.search(code, 0, i + 1)
        return m.group(0) if m else None


def literal(argument: str, masked: str) -> bool:
    """`argument` is one string literal (or a run of them) and nothing else."""
    return argument.strip() != "" and masked.strip() == ""


def check_file(path: Path, display: str) -> tuple[list[str], int, int]:
    text = path.read_text(encoding="utf-8", errors="replace")
    if NAME not in text and "sqlx" not in text:
        return [], 0, 0
    lines = text.split("\n")
    masked = code_mask(text)
    code = masked.split("\n")
    starts = [0]
    for line in lines:
        starts.append(starts[-1] + len(line) + 1)

    def line_of(offset: int) -> int:
        lo, hi = 0, len(starts) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if starts[mid] <= offset:
                lo = mid
            else:
                hi = mid - 1
        return lo

    problems: list[str] = []
    calls = 0
    for index, line in enumerate(code):
        for m in re.finditer(rf"\b{NAME}\b(\s*\()?", line):
            where = f"{display}:{index + 1}"
            if not m.group(1):
                problems.append(f"{where}: `{NAME}` outside a call by path (rule 2)")
                continue
            calls += 1
            if not has_reason(lines, code, index, m.end()):
                problems.append(
                    f"{where}: `{NAME}(` with no `// Reason:` beside it (rule 1)"
                )

    # Rule 4: every mention of a builder type is one rule 3 can follow.
    for m in re.finditer(BUILDER, masked):
        before = masked[: m.start()]
        after = masked[m.start() :]
        if CONSTRUCTOR.match(after) or re.search(
            r"\b\w+\s*:\s*(?:&\s*(?:'\w+\s+)?(?:mut\s+)?)?$", before
        ):
            continue
        problems.append(
            f"{display}:{line_of(m.start()) + 1}: `{m.group(0)}` where this gate cannot "
            "follow the builder: name it in a binding, parameter or field type, or call "
            "its constructor (rule 4)"
        )
    # A builder type reached through an import never spells `sqlx::QueryBuilder` above:
    # `use sqlx::{QueryBuilder, ..}`, `use sqlx::query_builder`, a glob, or sqlx renamed.
    for m in re.finditer(r"\b(?:pub(?:\([^)]*\))?\s+)?use\s+sqlx\b[^;]*;", masked):
        statement = m.group(0)
        if re.search(
            r"\b(?:QueryBuilder|Separated|query_builder)\b|\*|^\S*\s*use\s+sqlx\s+as\b"
            r"|\bself\s+as\b",
            statement,
        ):
            problems.append(
                f"{display}:{line_of(m.start()) + 1}: an import that can bring a sqlx "
                "builder type in under a name this gate does not follow (rule 4)"
            )
    for m in re.finditer(r"\bextern\s+crate\s+sqlx\s+as\b", masked):
        problems.append(
            f"{display}:{line_of(m.start()) + 1}: sqlx renamed, so its builder type "
            "reaches code under a name this gate does not follow (rule 4)"
        )

    builders = set(TYPED_NAME.findall(masked)) | set(
        BOUND_TO_CONSTRUCTOR.findall(masked)
    )
    while True:
        grown = builders | {
            name
            for name, chain in BOUND_TO_SEPARATED.findall(masked)
            if receiver(chain + ".", len(chain)) in builders
        }
        if grown == builders:
            break
        builders = grown

    texts = 0
    sites: list[tuple[int, int, str]] = []
    for m in CONSTRUCTOR.finditer(masked):
        sites.append((m.end() - 1, m.start(), f"QueryBuilder::{m.group(1)}"))
    for m in TEXT_METHOD.finditer(masked):
        if receiver(masked, m.start()) in builders:
            sites.append((m.end() - 1, m.start(1), m.group(1)))
    for open_at, at, what in sites:
        texts += 1
        close_at = closing(masked, open_at)
        inner = slice(open_at + 1, close_at)
        # The constructor's text is its first argument; `push` has only one.
        argument, argument_masked = text[inner], masked[inner]
        if what.endswith("with_arguments"):
            argument = argument_masked = "x"
        else:
            depth, cut = 0, len(argument_masked)
            for i, c in enumerate(argument_masked):
                if c in "([{":
                    depth += 1
                elif c in ")]}":
                    depth -= 1
                elif c == "," and depth == 0:
                    cut = i
                    break
            argument, argument_masked = argument[:cut], argument_masked[:cut]
        if literal(argument, argument_masked):
            continue
        index = line_of(at)
        if not has_reason(lines, code, index, at - starts[index]):
            problems.append(
                f"{display}:{index + 1}: `{what}(` given text that is not a string literal, "
                "with no `// Reason:` beside it (rule 3)"
            )
    return problems, calls, texts


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
    calls = texts = 0
    for path in files:
        found, n, m = check_file(path, str(path.relative_to(root)))
        problems += found
        calls += n
        texts += m
    if not fixture_root and calls == 0:
        problems.append(
            f"no `{NAME}(` call found under crates/, tools/ or examples/ (rule 5)"
        )
    if not fixture_root and texts == 0:
        problems.append(
            "no text handed to a `sqlx::QueryBuilder` found under crates/, tools/ or "
            "examples/ (rule 5)"
        )
    if problems:
        print("sql-safe assertions: FAILED", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(
        f"sql-safe assertions: ok ({calls} calls, each with its reason; "
        f"{texts} builder texts, each a literal or with its reason)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

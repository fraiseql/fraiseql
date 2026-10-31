#!/usr/bin/env python3
"""A `fraiseql_query` example passes its values as variables, never in the query text.

The bridge's second argument binds GraphQL variables
(`fraiseql_query(graphql: string, variables: string)`). An example that builds the
query by interpolation, `` `{ price(sku: "${sku}") { amount } }` ``, lets a crafted
value change the document, and examples and test fixtures are what new code copies.
So every `fraiseql_query(` call under docs/, examples/, tests/ and any `tests/`
directory below crates/ is found, its first argument read past whitespace and
comments, and a template literal holding an unescaped `${` there fails the gate. A `${` in the variables argument is fine: that one is data.

A tree with no call at all also fails: a gate that found nothing to check proved
nothing.

Overrides, for testing: QUERY_VARIABLES_ROOT=<dir>.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

CALL = "fraiseql_query("
SUFFIXES = {".md", ".ts", ".mts", ".tsx", ".js", ".mjs"}
SKIP_DIRS = {"node_modules", "target", ".git"}


def skip_trivia(text: str, i: int) -> int:
    """The index of the first character at or after `i` that is not space or a comment."""
    while i < len(text):
        if text[i].isspace():
            i += 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            i = len(text) if end < 0 else end + 1
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = len(text) if end < 0 else end + 2
        else:
            break
    return i


def interpolates(text: str, i: int) -> bool:
    """Whether the template literal opening at `i` holds an unescaped `${`."""
    j = i + 1
    while j < len(text):
        c = text[j]
        if c == "\\":
            j += 2
        elif c == "`":
            return False
        elif text.startswith("${", j):
            return True
        else:
            j += 1
    return False


def scan(path: Path, rel: str) -> tuple[int, list[str]]:
    """(calls found, one error per call whose query is interpolated) for one file."""
    text = path.read_text(encoding="utf-8", errors="replace")
    calls, errors = 0, []
    start = text.find(CALL)
    while start >= 0:
        calls += 1
        arg = skip_trivia(text, start + len(CALL))
        if arg < len(text) and text[arg] == "`" and interpolates(text, arg):
            line = text.count("\n", 0, start) + 1
            errors.append(
                f"{rel}:{line}: fraiseql_query interpolates a value into the query text; "
                "bind it as a variable (`query ($sku: String!) { price(sku: $sku) }` "
                "with JSON.stringify({ sku }) as the second argument)"
            )
        start = text.find(CALL, start + len(CALL))
    return calls, errors


def files(root: Path):
    """Every checked file under docs/, examples/, tests/ and any `tests/` below crates/."""
    for top in ("docs", "examples", "tests", "crates"):
        base = root / top
        if not base.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
            if top == "crates" and "tests" not in Path(dirpath).relative_to(base).parts:
                continue
            for name in sorted(filenames):
                path = Path(dirpath) / name
                if path.suffix in SUFFIXES:
                    yield path


def main() -> int:
    root = Path(os.environ.get("QUERY_VARIABLES_ROOT") or Path(__file__).resolve().parent.parent)
    calls, holders, errors = 0, 0, []
    for path in files(root):
        found, errs = scan(path, path.relative_to(root).as_posix())
        calls += found
        holders += bool(found)
        errors.extend(errs)
    if errors:
        print("\n".join(errors))
        print(f"query variables: {len(errors)} of {calls} calls interpolate into the query")
        return 1
    if calls == 0:
        print("query variables: no fraiseql_query call found under docs/, examples/ or tests/")
        return 1
    print(f"query variables: ok ({calls} calls in {holders} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Each workspace crate's production code stays within its line budget (#1882).

The budget is a runaway-growth ratchet on **production code**: the non-blank,
non-comment lines of a crate's `src/` that are compiled outside `cfg(test)`. Unit
tests, docs and the crate's `tests/` are counted and reported, never gated, so a
new test does not cost the budget a new feature does, and a guard is not pushed
out of `src/` to make room.

A line of `src/` is, in this order of precedence:

1. **test**, when its file is a test file or the line lies in a test item.
   A test file is one named `tests.rs`, `*_tests.rs` or `test_*.rs`, or below a
   directory named `tests`, `*_tests` or `test_*`; one declared by a test-only
   `mod name;` (with or without `#[path = "…"]`), with everything below that
   module's directory; or one opening with `#![cfg(test)]`. A test item is any item
   under a test-only `#[cfg(…)]` attribute, from the attribute to the item's end.
   Test-only means `cfg(test)` or `cfg(all(…, test, …))`: `cfg(any(test, …))`
   and `cfg(not(test))` are compiled into a production build, and count as
   production.
2. **blank**, when it holds nothing but whitespace.
3. **comment**, when everything on it is comment: `//`, `///`, `//!` or the inside
   of a `/* … */`. A line inside a string literal is code.
4. **production**, otherwise.

Budgets are [workspace.metadata.crate-size-budget] in the root Cargo.toml. A crate
over its budget fails, and so does a crate with a `src/` and no budget row.

Usage:
  check-crate-sizes.py               every crate
  check-crate-sizes.py <crate>       one crate
  check-crate-sizes.py --files <crate>
                                     each file's production, comment, blank and
                                     test lines, as the counting rules see them

Overrides, for testing: CRATE_SIZES_ROOT=<dir>.
"""

from __future__ import annotations

import os
import re
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(os.environ.get("CRATE_SIZES_ROOT") or Path(__file__).resolve().parent.parent)
WARN_PERCENT = 85


def is_test_name(name: str) -> bool:
    """A file stem or directory name that marks test code."""
    return name == "tests" or name.endswith("_tests") or name.startswith("test_")


LEXEME = re.compile(r"//|/\*|b?r#*\"|[\"']")
RAW_STRING = re.compile(r'b?r(#*)"')
CHAR = re.compile(r"'(\\.[^']{0,9}|[^\\'\n])'")


def mask(text: str) -> tuple[str, str]:
    """(code, kinds): `code` is `text` with comments and the inside of string and
    char literals blanked to spaces, and `kinds` marks each character `c` (comment),
    `s` (literal content) or `x` (code). Newlines are kept, so offsets and line
    numbers match `text`."""
    code, kinds = list(text), ["x"] * len(text)
    i, n = 0, len(text)

    def blank(start: int, end: int, kind: str) -> None:
        for k in range(start, end):
            if text[k] != "\n":
                code[k] = " "
            kinds[k] = kind

    while i < n:
        nxt = LEXEME.search(text, i)
        if not nxt:
            break
        i = nxt.start()
        c = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            blank(i, end, "c")
            i = end
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(i, j, "c")
            i = j
        elif (m := RAW_STRING.match(text, i)) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + m.group(1)
            end = text.find(close, m.end())
            end = n if end < 0 else end
            blank(m.end(), end, "s")
            i = end + len(close)
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            blank(i + 1, min(j, n), "s")
            i = j + 1
        elif c == "'" and (m := CHAR.match(text, i)):
            blank(i + 1, m.end() - 1, "s")
            i = m.end()
        else:
            i += 1
    return "".join(code), "".join(kinds)


def split_args(pred: str) -> list[str]:
    """The top-level comma-separated arguments of a cfg predicate's call."""
    args, depth, cur = [], 0, ""
    for ch in pred:
        if ch == "," and depth == 0:
            args.append(cur.strip())
            cur = ""
            continue
        depth += (ch == "(") - (ch == ")")
        cur += ch
    if cur.strip():
        args.append(cur.strip())
    return args


def test_only(pred: str) -> bool:
    """Whether a cfg predicate holds only in a test build."""
    pred = re.sub(r"\s+", "", pred)
    if pred == "test":
        return True
    m = re.fullmatch(r"all\((.*)\)", pred)
    return bool(m) and any(test_only(arg) for arg in split_args(m.group(1)))


def balanced_end(code: str, start: int) -> int:
    """The index just past the bracket group opening at `code[start]`."""
    depth = 0
    for j in range(start, len(code)):
        if code[j] in "([{":
            depth += 1
        elif code[j] in ")]}":
            depth -= 1
            if depth == 0:
                return j + 1
    return len(code)


def item_end(code: str, start: int) -> int:
    """The index just past the item starting at or after `start`: its `;` at depth
    0, or the `}` that closes its body."""
    depth = 0
    for j in range(start, len(code)):
        ch = code[j]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
            if depth == 0 and ch == "}":
                return j + 1
        elif ch == ";" and depth == 0:
            return j + 1
    return len(code)


ATTR = re.compile(r"#\s*\[")
CFG = re.compile(r"#\s*\[\s*cfg\s*\(")
INNER_CFG = re.compile(r"#\s*!\s*\[\s*cfg\s*\(")
MOD_DECL = re.compile(r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(r#)?(\w+)\s*;")
PATH_ATTR = re.compile(r'#\s*\[\s*path\s*=\s*"([^"]*)"\s*\]')


@dataclass
class FileScan:
    test_lines: set[int] = field(default_factory=set)
    whole_file: bool = False
    test_modules: list[Path] = field(default_factory=list)


def scan(path: Path, text: str, code: str) -> FileScan:
    """The test items of one file, and the files its test-only `mod` declarations name."""
    out = FileScan()
    head = code.lstrip()
    m = INNER_CFG.match(head)
    if m:
        open_paren = len(code) - len(head) + m.end() - 1
        if test_only(code[open_paren + 1 : balanced_end(code, open_paren) - 1]):
            out.whole_file = True
            return out
    for m in CFG.finditer(code):
        open_paren = m.end() - 1
        close = balanced_end(code, open_paren)
        if not test_only(code[open_paren + 1 : close - 1]):
            continue
        attr_end = balanced_end(code, code.index("[", m.start()))
        # The item: past any further attributes, which may carry `#[path = "…"]`.
        j, attrs = attr_end, text[m.start() : attr_end]
        while True:
            rest = ATTR.match(code, j + len(code[j:]) - len(code[j:].lstrip()))
            if not rest:
                break
            k = balanced_end(code, code.index("[", rest.start()))
            attrs += text[rest.start() : k]
            j = k
        end = item_end(code, j)
        first, last = code.count("\n", 0, m.start()), code.count("\n", 0, max(end - 1, 0))
        out.test_lines.update(range(first, last + 1))
        decl = MOD_DECL.match(code, j)
        if decl and decl.end() == end:
            name = decl.group(2)
            if path_attr := PATH_ATTR.search(attrs):
                out.test_modules.append(path.parent / path_attr.group(1))
            else:
                base = path.parent if path.name in ("mod.rs", "lib.rs", "main.rs") else path.parent / path.stem
                out.test_modules += [base / f"{name}.rs", base / name]
    return out


@dataclass
class Counts:
    production: int = 0
    comment: int = 0
    blank: int = 0
    test: int = 0

    def add(self, other: Counts) -> None:
        self.production += other.production
        self.comment += other.comment
        self.blank += other.blank
        self.test += other.test


def classify(text: str, code: str, kinds: str, test_lines: set[int], whole: bool) -> Counts:
    counts = Counts()
    offset = 0
    for number, line in enumerate(text.split("\n")):
        line_kinds = kinds[offset : offset + len(line)]
        offset += len(line) + 1
        if whole or number in test_lines:
            counts.test += 1
        elif not line.strip():
            counts.blank += 1
        elif all(k == "c" or ch.isspace() for ch, k in zip(line, line_kinds)):
            counts.comment += 1
        else:
            counts.production += 1
    if text.endswith("\n"):
        # The empty string after the final newline is not a line.
        if whole or (text.count("\n") in test_lines):
            counts.test -= 1
        else:
            counts.blank -= 1
    return counts


def crate_files(src: Path) -> dict[Path, Counts]:
    """Every `.rs` file under `src`, with its counts."""
    files = sorted(src.rglob("*.rs"))
    scans: dict[Path, tuple[str, str, str, FileScan]] = {}
    test_paths: set[Path] = set()
    for path in files:
        text = path.read_text(encoding="utf-8", errors="replace")
        code, kinds = mask(text)
        found = scan(path, text, code)
        scans[path] = (text, code, kinds, found)
        test_paths.update(p.resolve() for p in found.test_modules)

    def is_test_file(path: Path) -> bool:
        rel = path.relative_to(src)
        if is_test_name(path.stem) or any(is_test_name(d) for d in rel.parts[:-1]):
            return True
        resolved = path.resolve()
        return any(resolved == t or t in resolved.parents for t in test_paths)

    result = {}
    for path, (text, code, kinds, found) in scans.items():
        whole = found.whole_file or is_test_file(path)
        result[path] = classify(text, code, kinds, found.test_lines, whole)
    return result


def total_lines(directory: Path) -> int:
    if not directory.is_dir():
        return 0
    return sum(len(p.read_text(encoding="utf-8", errors="replace").splitlines()) for p in directory.rglob("*.rs"))


def budgets() -> dict[str, int]:
    with (ROOT / "Cargo.toml").open("rb") as f:
        meta = tomllib.load(f).get("workspace", {}).get("metadata", {})
    return {k: int(v) for k, v in meta.get("crate-size-budget", {}).items()}


def main(argv: list[str]) -> int:
    if argv[:1] == ["--files"] and len(argv) == 2:
        src = ROOT / "crates" / argv[1] / "src"
        for path, c in crate_files(src).items():
            print(f"{path.relative_to(src).as_posix()} production={c.production} comment={c.comment} blank={c.blank} test={c.test}")
        return 0
    only = argv[0] if argv else None
    table = budgets()
    crates = sorted(p.name for p in (ROOT / "crates").iterdir() if (p / "src").is_dir())
    failures, checked = [], 0
    print(f"{'Crate':<26} {'Production':>10} {'Budget':>8} {'Status':<10} {'Docs':>8} {'Unit tests':>10} {'tests/':>8}")
    for name in crates:
        if only and name != only:
            continue
        total = Counts()
        for counts in crate_files(ROOT / "crates" / name / "src").values():
            total.add(counts)
        integration = total_lines(ROOT / "crates" / name / "tests")
        budget = table.get(name)
        if budget is None:
            status = "NO BUDGET"
            failures.append(f"{name} has a src/ directory but no row in [workspace.metadata.crate-size-budget]")
        elif total.production > budget:
            status = "OVER"
            failures.append(f"{name}: {total.production} production lines, over its budget of {budget} by {total.production - budget}")
        elif total.production * 100 > budget * WARN_PERCENT:
            status = f"{total.production * 100 // budget}%"
        else:
            status = "ok"
        checked += 1
        shown = "-" if budget is None else str(budget)
        print(f"{name:<26} {total.production:>10} {shown:>8} {status:<10} {total.comment:>8} {total.test:>10} {integration:>8}")
    print()
    if only and checked == 0:
        print(f"crate sizes: no crate named {only} has a src/ directory")
        return 1
    if failures:
        print("\n".join(failures))
        print(f"crate sizes: {len(failures)} problem(s). A budget counts production lines only (see this")
        print("script's header); raise one in Cargo.toml with a comment saying why.")
        return 1
    print(f"crate sizes: ok ({checked} crates within budget; docs, unit tests and tests/ are reported, not gated)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

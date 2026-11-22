#!/usr/bin/env python3
"""Every DDL script FraiseQL runs takes the workspace's one DDL lock.

WHY THIS GATE EXISTS
--------------------
The scripts that create FraiseQL's own objects are idempotent (`CREATE … IF NOT
EXISTS`), and idempotent is not concurrency-safe: two callers at once both pass the
existence check, and one loses on a catalogue unique index. The cure is one
transaction-scoped advisory lock, `fraiseql_guard::ddl::MIGRATION_LOCK_KEY`, taken
before the script's first statement: `lock_prefixed` for a multi-statement script run as
one simple query, `locked_statement` for one statement sent on its own. The catalogue
indexes are shared by every script, so the key is one for all of them.

The lock was missing from a dozen runners, one crate at a time, each a plain
`raw_sql(SCHEMA_SQL)` that reads as finished. This gate makes a new one visible where it
is written.

RULES
-----
1. A call that executes SQL text (`raw_sql`, `batch_execute`, `simple_query`,
   `execute_raw_query`, `query`, `query_one`, `query_opt`, `query_scalar`, `query_as`,
   `execute`, `prepare`), or a function of this tree that hands its own parameter to one,
   whose text is DDL, takes that text through `lock_prefixed(..)` or
   `locked_statement(..)` as its outermost call. Text is DDL when it holds a `CREATE
   SCHEMA|TABLE|INDEX|TYPE|POLICY|FUNCTION|VIEW|TRIGGER|EXTENSION|SEQUENCE|…` or `ALTER
   TABLE|…` in a string literal, in a `format!` / `concat!` of one, in an
   `include_str!`'d file, or in a constant, or a function returning text, that does. A
   local binding is followed to what it is bound to.
2. A `pg_advisory_lock` / `pg_advisory_xact_lock` statement binds `MIGRATION_LOCK_KEY`:
   a second key would not exclude the scripts under the first.
3. Either rule is waived for one statement by a comment in the block directly above it
   saying why: `// No DDL lock: <why>` for rule 1, `// Not the DDL lock: <why>` for rule
   2. A `// Reason:` does not waive either: the SQL-text gate requires one on every
   `AssertSqlSafe`, so it would waive every DDL text built at runtime.
4. Scanning the tree finds at least one locked DDL execution and at least one statement
   rule 2 accepts. Finding none means the pattern this gate reads has moved.

Only non-test code is scanned: files under `tests/`, `tests.rs`, `*_tests.rs`, and items
under `#[cfg(test)]` are skipped. Comments and string literals never count as calls.

`DDL_LOCK_ROOT` points the gate at another directory (every `.rs` under it), for its
tests; rule 4 then does not apply.
"""

from __future__ import annotations

import bisect
import importlib.util
import os
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location(
    "sql_safe", HERE / "check-sql-safe-assertions.py"
)
_sql_safe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_sql_safe)
closing = _sql_safe.closing
tracked_sources = _sql_safe.tracked_sources

DDL = re.compile(
    r"\b(?:CREATE\s+(?:OR\s+REPLACE\s+)?(?:UNIQUE\s+)?(?:TEMP(?:ORARY)?\s+)?"
    r"(?:SCHEMA|TABLE|INDEX|TYPE|POLICY|FUNCTION|PROCEDURE|VIEW|MATERIALIZED\s+VIEW|"
    r"TRIGGER|EXTENSION|SEQUENCE|DOMAIN)"
    r"|ALTER\s+(?:TABLE|SCHEMA|TYPE|SEQUENCE|FUNCTION|VIEW|DOMAIN))\b",
    re.IGNORECASE,
)
EXECUTORS = (
    "raw_sql|batch_execute|simple_query|execute_raw_query|query|query_one|query_opt|"
    "query_scalar|query_as|execute|prepare"
)
EXECUTOR_CALL = re.compile(rf"(?<![\w:])(?:\w+::)*({EXECUTORS})\s*\(")
LOCK_CALL = re.compile(r"^(?:(?:::)?\w+::)*(lock_prefixed|locked_statement)\s*\(")
TAIL = re.compile(
    r"^(?:\?|\.\s*(?:as_str|as_ref|to_string|into|unwrap|clone)\s*\(\s*\)"
    r"|\.\s*(?:map_err|expect|context|with_context)\s*\()"
)
PG_LOCK = re.compile(r"\bpg_advisory(?:_xact)?_lock\s*\(")
NO_LOCK = re.compile(r"//\s*No DDL lock:\s*\S")
NOT_THE_LOCK = re.compile(r"//\s*Not the DDL lock:\s*\S")
WRITER = re.compile(r"&\s*mut\s+String\b")
TEXT_RETURN = re.compile(r"->\s*(?:&\s*(?:'\w+\s+)?str|String|Result\s*<\s*String|Cow\b)")


TOKEN = re.compile(r"//|/\*|b?r#*\"|b?\"|'")
STRING = re.compile(r'"(?:[^"\\]|\\.)*"', re.DOTALL)
CHAR = re.compile(r"'(?:\\.[^']*|[^\\'])'")


def lex(text: str) -> tuple[str, list[tuple[int, int, str]]]:
    """`text` with every comment and string literal blanked (newlines kept), and every
    string literal's span and body."""
    out, found, i, n = [], [], 0, len(text)

    def blank(s: str) -> str:
        return re.sub(r"[^\n]", " ", s)

    while i < n:
        m = TOKEN.search(text, i)
        if not m:
            out.append(text[i:])
            break
        at, token = m.start(), m.group(0)
        if token[0] in "br" and at and (text[at - 1].isalnum() or text[at - 1] == "_"):
            out.append(text[i : at + 1])
            i = at + 1
            continue
        out.append(text[i:at])
        if token == "//":
            j = text.find("\n", at)
            j = n if j < 0 else j
        elif token == "/*":
            depth, j = 1, at + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
        elif token.endswith('"') and "r" in token:
            close = '"' + token[token.index("r") + 1 : -1]
            k = text.find(close, m.end())
            k = n if k < 0 else k
            found.append((at, k + len(close), text[m.end() : k]))
            j = k + len(close)
        elif token.endswith('"'):
            q = at + len(token) - 1
            sm = STRING.match(text, q)
            j = sm.end() if sm else n
            found.append((at, j, text[q + 1 : j - 1]))
        else:
            cm = CHAR.match(text, at)
            if not cm:
                out.append("'")
                i = at + 1
                continue
            j = cm.end()
        out.append(blank(text[at:j]))
        i = j
    return "".join(out), found


def test_regions(masked: str) -> list[tuple[int, int]]:
    """The spans of the items under `#[cfg(test)]`."""
    regions = []
    for m in re.finditer(r"#\[cfg\(test\)\]", masked):
        brace, semi = masked.find("{", m.end()), masked.find(";", m.end())
        if brace >= 0 and (semi < 0 or brace < semi):
            regions.append((m.start(), closing(masked, brace) + 1))
        elif semi >= 0:
            regions.append((m.start(), semi + 1))
    return regions


class Source:
    """One file: its text, the text with comments and literals blanked, its literals, and
    the spans of its test items, functions and constants."""

    def __init__(self, path: Path, display: str) -> None:
        self.path, self.display = path, display
        self.text = path.read_text(encoding="utf-8", errors="replace")
        self.masked, self.literals = lex(self.text)
        self.ddl_literals = [s for s, _, body in self.literals if DDL.search(body)]
        self.tests = test_regions(self.masked)
        self.lines = self.text.split("\n")
        self.code_lines = self.masked.split("\n")
        self.fns: list[tuple[str, int, int, int, int, str]] = []
        for m in re.finditer(r"\bfn\s+(\w+)\s*(?:<[^(]*?>)?\s*\(", self.masked):
            params_end = closing(self.masked, m.end() - 1)
            if params_end < 0:
                continue
            brace = self.masked.find("{", params_end)
            semi = self.masked.find(";", params_end)
            if brace < 0 or (0 <= semi < brace):
                continue
            body_end = closing(self.masked, brace)
            signature = self.masked[params_end:brace]
            self.fns.append(
                (m.group(1), m.end(), params_end, brace, body_end, signature)
            )
        # `use path::{NAME as ALIAS}`: an alias is as much the constant as its name.
        self.aliases = re.findall(r"\b(\w+)\s+as\s+(\w+)\s*[,}; ]", self.masked)
        self.consts: list[tuple[str, int, int]] = []
        for m in re.finditer(r"\b(?:const|static)\s+([A-Z_][A-Z0-9_]*)\s*:[^=;]*=", self.masked):
            end = self.statement_end(m.end())
            self.consts.append((m.group(1), m.end(), end))

    def statement_end(self, start: int) -> int:
        depth, i = 0, start
        while i < len(self.masked):
            c = self.masked[i]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                if depth == 0:
                    return i
                depth -= 1
            elif c == ";" and depth == 0:
                return i
            i += 1
        return i

    def in_test(self, at: int) -> bool:
        return any(s <= at < e for s, e in self.tests)

    def line_of(self, at: int) -> int:
        return self.text.count("\n", 0, at)

    def enclosing_fn(self, at: int):
        best = None
        for fn in self.fns:
            if fn[3] < at < fn[4] and (best is None or fn[3] > best[3]):
                best = fn
        return best

    def literal_ddl(self, start: int, end: int) -> bool:
        k = bisect.bisect_left(self.ddl_literals, start)
        if k < len(self.ddl_literals) and self.ddl_literals[k] < end:
            return True
        for m in re.finditer(r"include_str!\s*\(", self.masked[start:end]):
            lit = next((l for l in self.literals if l[0] >= start + m.end()), None)
            if lit:
                included = self.path.parent / lit[2]
                if included.is_file() and DDL.search(included.read_text(errors="replace")):
                    return True
        return False

    def waived(self, at: int, marker: re.Pattern[str]) -> bool:
        index = self.line_of(at)
        start = index
        while start > 0 and (
            self.code_lines[start - 1].rstrip().endswith(("(", "=", ",", "."))
            or self.code_lines[start].lstrip().startswith((".", "?", ")"))
        ):
            start -= 1
        if marker.search(self.lines[index]):
            return True
        above = start - 1
        while above >= 0 and self.lines[above].strip().startswith("//"):
            if marker.search(self.lines[above]):
                return True
            above -= 1
        return False


class Tree:
    def __init__(self, sources: list[Source]) -> None:
        self.sources = sources
        self.ddl_consts: set[str] = set()
        self.ddl_fns: set[str] = set()
        # name -> parameter positions it hands, unlocked, to an executor
        self.pass_through: dict[str, set[int]] = {}
        changed = True
        while changed:
            changed = False
            for src in sources:
                for name, alias in src.aliases:
                    for known in (self.ddl_consts, self.ddl_fns):
                        if name in known and alias not in known:
                            known.add(alias)
                            changed = True
                for name, start, end in src.consts:
                    if name not in self.ddl_consts and self.text_ddl(src, start, end, None):
                        self.ddl_consts.add(name)
                        changed = True
                for fn in src.fns:
                    name, params_start, params_end, brace, body_end, signature = fn
                    # A function returning text, or one writing into a `&mut String` its
                    # caller returns: `generate(&mut sql, ..)` builds the caller's DDL.
                    if (
                        name not in self.ddl_fns
                        and (
                            TEXT_RETURN.search(signature)
                            or WRITER.search(src.masked[params_start:params_end])
                        )
                        and not src.in_test(brace)
                        and self.text_ddl(src, brace, body_end, None)
                    ):
                        self.ddl_fns.add(name)
                        changed = True

    def text_ddl(self, src: Source, start: int, end: int, fn, depth: int = 0) -> bool:
        """Whether the expression `start..end` of `src` is (or builds) DDL text."""
        if depth > 6:
            return False
        if src.literal_ddl(start, end):
            return True
        code = src.masked[start:end]
        if any(name in self.ddl_consts for name in re.findall(r"\b([A-Z_][A-Z0-9_]+)\b", code)):
            return True
        if any(name in self.ddl_fns for name in re.findall(r"\b(\w+)\s*(?:::<[^>]*>)?\s*\(", code)):
            return True
        if fn is not None:
            for name in re.findall(r"(?<![\w.:])([a-z_]\w*)\b(?!\s*[(!:])", code):
                bound = self.binding(src, fn, name, start)
                if bound and self.text_ddl(src, bound[0], bound[1], fn, depth + 1):
                    return True
        return False

    @staticmethod
    def binding(src: Source, fn, name: str, before: int):
        """The expression the last `let name = …;` of `fn` before `before` binds."""
        body = src.masked[fn[3] : before]
        found = None
        for m in re.finditer(rf"\blet\s+(?:mut\s+)?{name}\b[^=;]*=(?!=)", body):
            found = m
        if not found:
            return None
        start = fn[3] + found.end()
        return start, src.statement_end(start)

    def locked(self, src: Source, start: int, end: int, fn, depth: int = 0) -> bool:
        """Whether the argument `start..end` takes its text through a lock call, outermost."""
        code = src.masked[start:end].strip()
        offset = start + (len(src.masked[start:end]) - len(src.masked[start:end].lstrip()))
        while True:
            stripped = code.lstrip("&*").lstrip()
            if stripped.startswith("mut "):
                stripped = stripped[4:].lstrip()
            offset += len(code) - len(stripped)
            code = stripped
            m = re.match(r"(?:(?:::)?\w+::)*AssertSqlSafe\s*\(", code)
            if not m:
                break
            inner_end = closing(src.masked, offset + m.end() - 1)
            if src.masked[inner_end + 1 : offset + len(code)].strip():
                return False
            offset += m.end()
            code = src.masked[offset:inner_end].strip()
        m = LOCK_CALL.match(code)
        if m:
            close = closing(src.masked, offset + m.end() - 1)
            rest = src.masked[close + 1 : offset + len(code)]
            while rest.strip():
                rest = rest.strip()
                t = TAIL.match(rest)
                if not t:
                    return False
                if rest[t.end() - 1] == "(":
                    at = offset + len(code) - len(rest) + t.end() - 1
                    rest = src.masked[closing(src.masked, at) + 1 : offset + len(code)]
                else:
                    rest = rest[t.end() :]
            return True
        if fn is not None and depth < 6 and re.fullmatch(r"[a-z_]\w*", code):
            bound = self.binding(src, fn, code, start)
            if bound:
                return self.locked(src, bound[0], bound[1], fn, depth + 1)
        return False


def arguments(src: Source, open_at: int) -> list[tuple[int, int]]:
    """The spans of the arguments of the call whose `(` is at `open_at`."""
    close = closing(src.masked, open_at)
    spans, depth, start = [], 0, open_at + 1
    for i in range(open_at + 1, close):
        c = src.masked[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == "," and depth == 0:
            spans.append((start, i))
            start = i + 1
    # The text, not the mask: an argument that is one string literal is blank in the mask.
    if src.text[start:close].strip():
        spans.append((start, close))
    return spans


def parameters(src: Source, fn) -> list[str]:
    """`fn`'s parameter names, `self` first when it takes one."""
    params = src.masked[fn[1] : fn[2]]
    names = [m.group(1) for m in re.finditer(r"(?:^|,)\s*(?:mut\s+)?(\w+)\s*:", params)]
    if re.match(r"\s*(?:&\s*(?:'\w+\s+)?)?(?:mut\s+)?self\b", params):
        names.insert(0, "self")
    return names


def calls(src: Source, tree: Tree):
    """Every executor call, and every call of a pass-through function, outside test code:
    (the call's name, its position, the spans of the arguments that carry text)."""
    for m in EXECUTOR_CALL.finditer(src.masked):
        if src.in_test(m.start()) or re.search(r"\bfn\s*$", src.masked[max(0, m.start() - 8) : m.start()]):
            continue
        args = arguments(src, m.end() - 1)
        if m.group(1) == "execute" and len(args) == 1:
            # sqlx's `.execute(executor)`: its one argument runs the query, it is no text.
            # tokio-postgres' `execute(text, params)` takes two.
            continue
        yield m.group(1), m.start(), args[:1]
    if tree.pass_through:
        names = "|".join(map(re.escape, tree.pass_through))
        for m in re.finditer(rf"(?<![\w])(?:\w+::|\.\s*)*({names})\s*\(", src.masked):
            if src.in_test(m.start()) or re.search(r"\bfn\s*$", src.masked[max(0, m.start() - 8) : m.start()]):
                continue
            args = arguments(src, m.end() - 1)
            receiver_call = src.masked[: m.start(1)].rstrip().endswith(".")
            picked = []
            for position in tree.pass_through[m.group(1)]:
                index = position - (1 if receiver_call else 0)
                if 0 <= index < len(args):
                    picked.append(args[index])
            yield m.group(1), m.start(), picked


def find_pass_through(tree: Tree) -> None:
    """Functions that hand a parameter, unlocked, to an executor (to a fixed point).

    Calls are matched to functions by name, so only a name defined once in the tree is
    followed: `new`, defined hundreds of times, would make every constructor a caller of
    whichever one passes its text on."""
    defined: dict[str, int] = {}
    for src in tree.sources:
        for fn in src.fns:
            defined[fn[0]] = defined.get(fn[0], 0) + 1
    changed = True
    while changed:
        changed = False
        for src in tree.sources:
            for name, at, args in list(calls(src, tree)):
                fn = src.enclosing_fn(at)
                if fn is None or src.in_test(at):
                    continue
                params = parameters(src, fn)
                for start, end in args:
                    code = src.masked[start:end].strip().lstrip("&*").strip()
                    code = re.sub(r"^(?:(?:::)?\w+::)*AssertSqlSafe\s*\((.*)\)$", r"\1", code).strip()
                    code = code.lstrip("&*").strip()
                    if (
                        code in params
                        and defined.get(fn[0]) == 1
                        and not tree.locked(src, start, end, fn)
                    ):
                        position = params.index(code)
                        if position not in tree.pass_through.setdefault(fn[0], set()):
                            tree.pass_through[fn[0]].add(position)
                            changed = True


def check(tree: Tree) -> tuple[list[str], int, int]:
    problems, locked, keyed = [], 0, 0
    for src in tree.sources:
        for name, at, args in calls(src, tree):
            fn = src.enclosing_fn(at)
            for start, end in args:
                if tree.locked(src, start, end, fn):
                    if tree.text_ddl(src, start, end, fn):
                        locked += 1
                    continue
                if tree.text_ddl(src, start, end, fn) and not src.waived(at, NO_LOCK):
                    problems.append(
                        f"{src.display}:{src.line_of(at) + 1}: `{name}(..)` runs DDL without "
                        "the workspace's DDL lock; take its text through "
                        "`fraiseql_guard::ddl::lock_prefixed` (a script) or `locked_statement` "
                        "(one statement), or say why not with `// No DDL lock: <why>` (rule 1)"
                    )
        for s, e, body in src.literals:
            if src.in_test(s) or not PG_LOCK.search(body):
                continue
            if src.display.replace("\\", "/").endswith("fraiseql-guard/src/ddl.rs"):
                keyed += 1
                continue
            # The statement: back to the last `;`, `{` or `}`, on to the next `;`, so that
            # a `.bind(..)` chained after the call is in it.
            stmt_start = max(src.masked.rfind(c, 0, s) for c in ";{}") + 1
            stmt_end = src.masked.find(";", e)
            stmt_end = len(src.masked) if stmt_end < 0 else stmt_end
            if "MIGRATION_LOCK_KEY" in src.masked[stmt_start:stmt_end]:
                keyed += 1
            elif not src.waived(s, NOT_THE_LOCK):
                problems.append(
                    f"{src.display}:{src.line_of(s) + 1}: an advisory lock on a key other "
                    "than `MIGRATION_LOCK_KEY`; bind that key, or say why this is not the "
                    "DDL lock with `// Not the DDL lock: <why>` (rule 2)"
                )
    return problems, locked, keyed


def is_test_path(relative: str) -> bool:
    parts = relative.replace("\\", "/").split("/")
    return (
        "tests" in parts
        or "testing" in parts
        or "benches" in parts
        or "examples" in parts
        or parts[-1] == "tests.rs"
        or parts[-1].endswith("_tests.rs")
        or "test_support" in parts[-1]
        or "fraiseql-test-support" in parts
        or "fraiseql-test-utils" in parts
    )


def main() -> int:
    fixture_root = os.environ.get("DDL_LOCK_ROOT")
    if fixture_root:
        root = Path(fixture_root)
        files = sorted(root.rglob("*.rs"))
    else:
        root = HERE.parent
        files = [p for p in tracked_sources(root) if str(p.relative_to(root)).startswith("crates/")]
    sources = []
    for path in files:
        display = str(path.relative_to(root))
        if not fixture_root and is_test_path(display):
            continue
        sources.append(Source(path, display))
    tree = Tree(sources)
    find_pass_through(tree)
    problems, locked, keyed = check(tree)
    if not fixture_root and locked == 0:
        problems.append("no DDL execution taking the DDL lock found under crates/ (rule 4)")
    if not fixture_root and keyed == 0:
        problems.append("no advisory lock on `MIGRATION_LOCK_KEY` found under crates/ (rule 4)")
    if problems:
        print("ddl lock: FAILED", file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"ddl lock: ok ({locked} DDL executions under the lock, {keyed} lock statements on its key)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

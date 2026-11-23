#!/usr/bin/env python3
"""Assert every authorization, session and tenancy guard site has a mutant and a killer.

`tools/guard-sites.toml` is the inventory. Each `[[site]]` names one place a rule is
applied: its file, the function it sits in, an exact `anchor` of code, the `mutant`
that replaces it, the `killers` (tests that must fail on the mutant) and the `shard`
(test command) they run in. `tools/run-guard-mutants.py` applies each mutant and runs
its shard; this gate checks the inventory against the tree, on every PR:

1. Every site resolves: its function is defined once in its file, outside test code;
   the anchor occurs exactly once inside that function (in a `.sql` file, which has no
   functions, a site names none and the anchor occurs once in the file); the mutant
   differs from it;
   the shard is declared; every killer names a test function defined in the shard's
   crate.
2. Every guard-shaped call is a site. Each `[[family]]` lists patterns for one kind of
   guard; every match in production code (test files, `tests/` directories and
   `#[cfg(test)]` modules excluded; comments blanked, and string contents too unless the
   family sets `scan = "strings"`, as an SQL predicate's family does; a function's own
   definition excluded) must lie inside some site's anchor, under a `[[held]]`
   path, or inside an `[[exempt]]` function. Each other match is reported at its file,
   line and column: a count would let a new unguarded site pass beside a removed one.
3. The scan proves it looked: it fails when a family pattern matches nothing, when no
   site resolves, and when the root holds no `crates/fraiseql-server`.
4. A held path that no longer exists fails, so a hold cannot outlive its code.

A `strings` family that sets `sql = true` also scans the `.sql` files under `crates/` and
`examples/` (migrations, and example schemas a test applies): SQL comments blanked, the
rest read as SQL.

Families are turned on one at a time as their sites are inventoried; a site may belong
to no enabled family (a rule with no call shape worth scanning for).

`GUARD_SITES_ROOT` sets the repository root, and `GUARD_SITES_INVENTORY` the inventory
path relative to it (for tools/tests/guard_sites_test.sh).
"""

from __future__ import annotations

import os
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

DEFAULT_INVENTORY = "tools/guard-sites.toml"
SCANNED = "crates"
SQL_SCANNED = ("crates", "examples")

SITE_KEYS = {"id", "file", "function", "anchor", "mutant", "killers", "shard", "joint"}
SITE_REQUIRED = SITE_KEYS - {"joint", "function"}
TOP_KEYS = {"family", "shard", "site", "held", "exempt"}
FAMILY_KEYS = {"name", "patterns", "scan", "sql"}

TEST_CFG_MOD = re.compile(
    r"#\[cfg\((?:test|all\([^\]]*\btest\b[^\]]*\))\)\]\s*"
    r"(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{"
)
RAW_STRING = re.compile(r'r(#*)"')
CHAR_LITERAL = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'")


def blank(text: str, keep_strings: bool = False, fill: str = " ") -> str:
    """Blank comments, char literals and (unless `keep_strings`) string contents,
    keeping every offset. String contents are blanked with `fill`."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        raw = RAW_STRING.match(text, i) if c == "r" and starts_token(text, i) else None
        if raw:
            close = '"' + raw.group(1)
            start = i + len(raw.group(0))
            stop = text.find(close, start)
            stop = n if stop == -1 else stop
            out.append(text[i:start])
            inner = text[start:stop]
            out.append(inner if keep_strings else "".join(ch if ch == "\n" else fill for ch in inner))
            out.append(text[stop : stop + len(close)])
            i = stop + len(close)
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j, n)
            inner = text[i + 1 : j]
            out.append('"' + (inner if keep_strings else "".join(ch if ch == "\n" else fill for ch in inner)))
            if j < n:
                out.append('"')
            i = j + 1
        elif c == "'" and (m := CHAR_LITERAL.match(text, i)):
            out.append(" " * len(m.group(0)))
            i = m.end()
        elif text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end == -1 else end
            out.append(" " * (end - i))
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
            out.append("".join(ch if ch == "\n" else " " for ch in text[i:j]))
            i = j
        else:
            out.append(c)
            i += 1
    return "".join(out)


def blank_sql(text: str) -> str:
    """Blank SQL comments (`--` to the end of the line, `/* */`), keeping every offset.
    A `'...'` literal is kept whole, so a `--` inside one is not a comment and dynamic DDL
    in a `format('...')` is read."""
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == "'":
            j = i + 1
            while j < n:
                if text[j] == "'":
                    if text.startswith("''", j):
                        j += 2
                        continue
                    break
                j += 1
            out.append(text[i : j + 1])
            i = j + 1
        elif text.startswith("--", i):
            end = text.find("\n", i)
            end = n if end == -1 else end
            out.append(" " * (end - i))
            i = end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            end = n if end == -1 else end + 2
            out.append("".join(ch if ch == "\n" else " " for ch in text[i:end]))
            i = end
        else:
            out.append(c)
            i += 1
    return "".join(out)


def starts_token(text: str, i: int) -> bool:
    """Whether `text[i]` starts a token: not inside an identifier (a `b` prefix allowed)."""
    before = text[i - 1] if i else " "
    if before == "b":
        before = text[i - 2] if i > 1 else " "
    return not (before.isalnum() or before == "_")


def closing_brace(code: str, open_at: int) -> int:
    """The offset just past the brace matching the `{` at `open_at` in blanked code."""
    depth = 0
    for j in range(open_at, len(code)):
        if code[j] == "{":
            depth += 1
        elif code[j] == "}":
            depth -= 1
            if depth == 0:
                return j + 1
    return len(code)


def test_module_spans(code: str) -> list[tuple[int, int]]:
    """The span of every `#[cfg(test)]` module of blanked code."""
    return [
        (match.start(), closing_brace(code, match.end() - 1))
        for match in TEST_CFG_MOD.finditer(code)
    ]


def blank_spans(text: str, spans: list[tuple[int, int]]) -> str:
    chars = list(text)
    for start, end in spans:
        for j in range(start, end):
            if chars[j] != "\n":
                chars[j] = " "
    return "".join(chars)



def is_test_path(path: Path) -> bool:
    return path.name.endswith("tests.rs") or "tests" in path.parts


def position(text: str, offset: int) -> str:
    line = text.count("\n", 0, offset) + 1
    column = offset - (text.rfind("\n", 0, offset) + 1) + 1
    return f"{line}:{column}"


@dataclass
class Source:
    raw: str
    code: str  # comments, strings and test modules blanked
    strings: str  # string contents only (outside test modules); all else is NUL

    @classmethod
    def of_sql(cls, raw: str) -> Source:
        """A `.sql` file: all of it is SQL, so both views are the text with its comments
        blanked."""
        sql = blank_sql(raw)
        return cls(raw, sql, sql)

    @classmethod
    def of(cls, raw: str) -> Source:
        code = blank(raw)
        tests = test_module_spans(code)
        # Everything outside a string is NUL, not a space, so no `\s` in a pattern can join
        # the end of one string to the start of the next: `contains("tenant_id") &&
        # contains("==")` is not a predicate.
        mask = blank(raw, fill="\1")
        contents = "".join(
            r if m == "\1" or r == "\n" else "\0" for r, m in zip(raw, mask, strict=True)
        )
        return cls(raw, blank_spans(code, tests), blank_spans(contents, tests))

    def function_spans(self, name: str) -> list[tuple[int, int]]:
        """The spans of `fn name` bodies, and of `const name` items (a guard can be a
        shared SQL predicate held in a const). `Type::name` keeps only those inside an
        `impl` block for `Type`, for a file that defines `name` more than once."""
        if "::" in name:
            owner, _, name = name.rpartition("::")
            blocks = []
            for match in re.finditer(r"\bimpl\b[^{;]*?\b" + re.escape(owner) + r"\b[^{;]*\{", self.code):
                blocks.append((match.start(), closing_brace(self.code, match.end() - 1)))
            return [
                span
                for span in self.function_spans(name)
                if any(a <= span[0] and span[1] <= b for a, b in blocks)
            ]
        spans = []
        for match in re.finditer(rf"\bfn\s+{re.escape(name)}\b", self.code):
            brace = self.code.find("{", match.end())
            semi = self.code.find(";", match.end())
            if brace == -1 or (semi != -1 and semi < brace):
                continue
            spans.append((match.start(), closing_brace(self.code, brace)))
        for match in re.finditer(rf"\bconst\s+{re.escape(name)}\s*:", self.code):
            semi = self.code.find(";", match.end())
            if semi != -1:
                spans.append((match.start(), semi + 1))
        return spans


class Gate:
    def __init__(self, root: Path, inventory: str) -> None:
        self.root = root
        self.inventory = inventory
        self.errors: list[str] = []
        self.sources: dict[str, Source | None] = {}

    def error(self, message: str) -> None:
        self.errors.append(message)

    def source(self, relative: str) -> Source | None:
        if relative not in self.sources:
            path = self.root / relative
            if not path.is_file():
                self.sources[relative] = None
            else:
                text = path.read_text(encoding="utf-8")
                self.sources[relative] = (
                    Source.of_sql(text) if relative.endswith(".sql") else Source.of(text)
                )
        return self.sources[relative]

    def load(self) -> dict | None:
        path = self.root / self.inventory
        try:
            with path.open("rb") as handle:
                data = tomllib.load(handle)
        except (OSError, tomllib.TOMLDecodeError) as exc:
            self.error(f"{self.inventory}: cannot be read: {exc}")
            return None
        for key in sorted(set(data) - TOP_KEYS):
            self.error(f"{self.inventory}: unknown table `{key}`")
        return data

    def check_sites(self, data: dict) -> list[tuple[str, int, int]]:
        """Resolve every site; return the (file, start, end) span of each resolved anchor."""
        shards = {}
        for shard in data.get("shard", []):
            name = shard.get("name")
            if not name or not shard.get("command") or not shard.get("crate"):
                self.error(f"{self.inventory}: shard {shard!r} needs `name`, `crate` and `command`")
                continue
            shards[name] = shard
        anchors = []
        seen: set[str] = set()
        joints: dict[str, list[str]] = {}
        for index, site in enumerate(data.get("site", [])):
            label = site.get("id", f"site #{index + 1}")
            unknown = set(site) - SITE_KEYS
            missing = SITE_REQUIRED - set(site)
            if unknown or missing:
                self.error(
                    f"{label}: unknown key(s) {sorted(unknown)}, missing key(s) {sorted(missing)}"
                )
                continue
            if label in seen:
                self.error(f"{label}: duplicate id")
            seen.add(label)
            if "joint" in site:
                joints.setdefault(site["joint"], []).append(label)
            source = self.source(site["file"])
            if source is None:
                self.error(f"{label}: {site['file']} does not exist")
                continue
            sql = site["file"].endswith(".sql")
            if sql == ("function" in site):
                self.error(
                    f"{label}: a site names its `function`, except in a `.sql` file, "
                    "which has none"
                )
                continue
            spans = [(0, len(source.raw))] if sql else source.function_spans(site["function"])
            if len(spans) != 1:
                self.error(
                    f"{label}: `fn {site['function']}` is defined {len(spans)} time(s) "
                    f"outside test code in {site['file']} (want 1)"
                )
                continue
            start, end = spans[0]
            body = source.raw[start:end]
            count = body.count(site["anchor"])
            if count != 1:
                where = site["file"] if sql else f"`fn {site['function']}` of {site['file']}"
                self.error(f"{label}: the anchor occurs {count} time(s) in {where} (want 1)")
                continue
            if site["mutant"] == site["anchor"]:
                self.error(f"{label}: the mutant is the anchor")
            at = start + body.index(site["anchor"])
            anchors.append((site["file"], at, at + len(site["anchor"])))
            shard = shards.get(site["shard"])
            if shard is None:
                self.error(f"{label}: shard `{site['shard']}` is not declared")
                continue
            if not site["killers"]:
                self.error(f"{label}: names no killer")
            for killer in site["killers"]:
                if not self.test_exists(shard["crate"], killer.rsplit("::", 1)[-1]):
                    self.error(
                        f"{label}: killer `{killer}` is not a test function in {shard['crate']}"
                    )
        for joint, members in sorted(joints.items()):
            if len(members) < 2:
                self.error(f"joint `{joint}` has one member ({members[0]}); a joint needs two")
        return anchors

    def test_exists(self, crate: str, name: str) -> bool:
        pattern = re.compile(rf"\bfn\s+{re.escape(name)}\s*[(<]")
        for path in (self.root / crate).rglob("*.rs"):
            if "target" in path.parts:
                continue
            if pattern.search(path.read_text(encoding="utf-8")):
                return True
        return False

    def scan(self, data: dict, anchors: list[tuple[str, int, int]]) -> None:
        held = []
        for entry in data.get("held", []):
            path = entry.get("path", "")
            if not path or not (self.root / path).exists():
                self.error(f"held path `{path}` does not exist; remove the hold")
            held.append(path.rstrip("/"))
        exempt: dict[str, list[tuple[int, int]]] = {}
        for entry in data.get("exempt", []):
            label = f"exempt {entry.get('file')}::{entry.get('function')}"
            if not entry.get("reason"):
                self.error(f"{label}: needs a `reason` (and the issue deciding it)")
            source = self.source(entry.get("file", ""))
            spans = source.function_spans(entry.get("function", "")) if source else []
            if len(spans) != 1:
                self.error(f"{label}: does not resolve to one function")
                continue
            exempt.setdefault(entry["file"], []).extend(spans)

        files = [
            path.relative_to(self.root)
            for path in sorted((self.root / SCANNED).rglob("*.rs"))
            if "target" not in path.parts
            and "src" in path.relative_to(self.root).parts
            and not is_test_path(path.relative_to(self.root))
        ]
        sql_files = [
            path.relative_to(self.root)
            for top in SQL_SCANNED
            for path in sorted((self.root / top).rglob("*.sql"))
            if "target" not in path.parts and not is_test_path(path.relative_to(self.root))
        ]
        if not (self.root / SCANNED / "fraiseql-server").is_dir():
            self.error(f"no {SCANNED}/fraiseql-server under {self.root}: the scan read the wrong tree")
        for family in data.get("family", []):
            name = family.get("name", "?")
            unknown = set(family) - FAMILY_KEYS
            scan = family.get("scan", "code")
            sql = family.get("sql", False)
            if (
                unknown
                or scan not in ("code", "strings")
                or not family.get("patterns")
                or sql not in (True, False)
                or (sql and scan != "strings")
            ):
                self.error(
                    f"family `{name}`: needs `name` and `patterns`, `scan` is \"code\" or "
                    f"\"strings\", `sql = true` only with `scan = \"strings\"`; unknown "
                    f"key(s) {sorted(unknown)}"
                )
                continue
            matched = 0
            for pattern in family.get("patterns", []):
                regex = re.compile(pattern)
                for relative in files + (sql_files if sql else []):
                    rel = relative.as_posix()
                    source = self.source(rel)
                    text = source.strings if scan == "strings" else source.code
                    for match in regex.finditer(text):
                        if re.search(r"\bfn\s+$", text[: match.start()][-16:]):
                            continue
                        matched += 1
                        covered = any(
                            f == rel and a <= match.start() and match.end() <= b
                            for f, a, b in anchors
                        ) or any(
                            rel == h or rel.startswith(h + "/") for h in held
                        ) or any(
                            a <= match.start() < b for a, b in exempt.get(rel, [])
                        )
                        if not covered:
                            self.error(
                                f"{rel}:{position(source.raw, match.start())}: {name} guard "
                                f"`{' '.join(match.group(0).split())}` is not an inventoried site"
                            )
            if matched == 0:
                self.error(
                    f"family `{name}` matched nothing in {len(files)} file(s); "
                    "the scan is not reading the guard sites"
                )


def main() -> int:
    root = Path(os.environ.get("GUARD_SITES_ROOT", Path(__file__).resolve().parent.parent))
    gate = Gate(root, os.environ.get("GUARD_SITES_INVENTORY", DEFAULT_INVENTORY))
    data = gate.load()
    anchors: list[tuple[str, int, int]] = []
    if data is not None:
        anchors = gate.check_sites(data)
        gate.scan(data, anchors)
        if not anchors:
            gate.error("no site resolved; the inventory is not reading this tree")
    if gate.errors:
        print("guard sites: FAILED")
        for error in gate.errors:
            print(f"  {error}")
        return 1
    families = len(data.get("family", []))
    print(f"guard sites: ok ({len(anchors)} site(s) resolved; {families} famil(y/ies) scanned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

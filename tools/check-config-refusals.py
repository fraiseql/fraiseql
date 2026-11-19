#!/usr/bin/env python3
"""Every refusal on the `fraiseql.toml` load path carries a diagnostic, or says why not.

WHY THIS GATE EXISTS
--------------------
A refused configuration exits 2 and names its code, file, table and key (#2034). The refusal
sites were converted where they were found: in the refusal functions and tables. Six more were
plain `bail!`s inside section checks (`[rest] max_embedded_reads`, `[observers] redis_url`,
`[[observers.handlers]]`, `[caching] redis_url`, `[security.rules]` and its two siblings,
`[inject_defaults]` against the schema document), so a key refused there exited 1 with prose
while its siblings exited 2 with a code. Nothing could tell the two apart.

`fraiseql run` had one more, outside the path first walked: a setting made in both
`fraiseql.toml` and the server's configuration file (#2055). `commands/run.rs` is in scope so
the next refusal written there as prose fails by name.

A `bail!` / `anyhow!` on that path is either a value check (a port of 0, an empty name, a rule
naming a query that does not exist), which keeps its prose by design, or a key refusal that
must be `refusal(...)`. This gate makes each site say which: every one is listed in
`tools/config-refusal-allowlist.txt` with the key it checks and why it carries no code.

The list is positional: a site is its file and the start of its message, never a count, so a
new key refusal written as `bail!` fails by name even when a value check is removed beside
it. An entry no site matches fails too: an allow-list that outlives its site hides the next.
"""

from __future__ import annotations

import os
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(os.environ.get("CONFIG_REFUSALS_ROOT", "."))
ALLOWLIST = pathlib.Path(
    os.environ.get("CONFIG_REFUSALS_ALLOWLIST", "tools/config-refusal-allowlist.txt")
)

# The `fraiseql.toml` load path: the configuration module, and the merger, where its
# settings meet the schema document. And `fraiseql run`, where its settings meet the
# server's configuration file: a setting made in both was refused there with prose only.
SCOPE = (
    "crates/fraiseql-cli/src/config",
    "crates/fraiseql-cli/src/schema/merger.rs",
    "crates/fraiseql-cli/src/commands/run.rs",
)

MACRO_RE = re.compile(r"\b(?:anyhow::)?(?:bail|anyhow)!\(")
PREFIX_LEN = 60


def sources() -> list[pathlib.Path]:
    out = []
    for scope in SCOPE:
        base = ROOT / scope
        files = [base] if base.is_file() else sorted(base.rglob("*.rs"))
        out.extend(
            f for f in files if not f.name.endswith("tests.rs") and "tests" not in f.parts
        )
    return out


def message_prefix(source: str, start: int) -> str:
    """The start of the macro's message, or its whole line when the message is not a literal.

    A message passed through (`anyhow!(message)`) says nothing of where it is raised, so the
    line that raises it stands for it.
    """
    rest = source[start:].lstrip()
    if rest.startswith('"'):
        body = rest[1:]
        end = min(
            (i for i in (body.find("\\"), body.find('"')) if i != -1), default=len(body)
        )
        return body[:end][:PREFIX_LEN].rstrip()
    line_start = source.rfind("\n", 0, start) + 1
    line_end = source.find("\n", start)
    return " ".join(source[line_start : line_end if line_end != -1 else None].split())


def sites() -> list[tuple[str, int, str]]:
    """(file, line, message prefix) for every macro in scope."""
    found = []
    for path in sources():
        source = path.read_text(errors="replace")
        for match in MACRO_RE.finditer(source):
            line = source.count("\n", 0, match.start()) + 1
            rel = path.relative_to(ROOT).as_posix()
            found.append((rel, line, message_prefix(source, match.end())))
    return found


def read_allowlist() -> tuple[list[tuple[str, str, str, str]], list[str]]:
    """(file, prefix, key, reason) per entry; and the malformed lines."""
    entries, malformed = [], []
    if not ALLOWLIST.is_file():
        return entries, malformed
    for raw in ALLOWLIST.read_text().splitlines():
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        fields = [f.strip() for f in raw.split(" | ")]
        if len(fields) != 4 or not all(fields):
            malformed.append(raw)
            continue
        entries.append((fields[0], fields[1], fields[2], fields[3]))
    return entries, malformed


def main() -> int:
    if "--list" in sys.argv:
        for file, line, prefix in sites():
            print(f"{file}:{line}  {prefix}")
        return 0

    found = sites()
    if not found:
        print("ERROR: no `bail!` / `anyhow!` found on the fraiseql.toml load path — this gate")
        print("is measuring nothing (a moved module, or a parser regression).")
        return 1
    entries, malformed = read_allowlist()

    seen: dict[tuple[str, str], int] = {}
    ambiguous = []
    for file, line, prefix in found:
        if (file, prefix) in seen:
            ambiguous.append(f"{file}:{seen[(file, prefix)]} and :{line}  {prefix}")
        seen[(file, prefix)] = line

    unlisted = [
        (file, line, prefix)
        for file, line, prefix in found
        if not any(file == e[0] and prefix == e[1] for e in entries)
    ]
    stale = [
        e for e in entries if not any(e[0] == file and e[1] == prefix for file, _, prefix in found)
    ]

    if ambiguous:
        print("ERROR: two sites in one file read alike, so one entry would stand for both:")
        for line in ambiguous:
            print(f"  {line}")
        print("Reword one message so each site is listed by itself.")
        return 1

    if malformed or unlisted or stale:
        if unlisted:
            print("ERROR: a refusal on the fraiseql.toml load path carries no diagnostic:")
            for file, line, prefix in unlisted:
                print(f"  {file}:{line}  {prefix}")
            print()
            print("A refused key is `crate::config::refusal(code, table, key, message)`, so it")
            print("exits 2 and names its code, table and key. A value check (a port of 0, an")
            print(f"empty name) is listed in {ALLOWLIST} as")
            print("`<file> | <message prefix> | <key it checks> | <why it carries no code>`.")
        if stale:
            print(f"ERROR: entries in {ALLOWLIST} that match no site (remove them):")
            for file, prefix, _, _ in stale:
                print(f"  {file} | {prefix}")
        if malformed:
            print(f"ERROR: malformed lines in {ALLOWLIST} (expected four ` | ` fields):")
            for line in malformed:
                print(f"  {line}")
        return 1

    print(
        f"OK: {len(found)} `bail!` / `anyhow!` sites on the fraiseql.toml load path, each a "
        f"value check listed with its key and reason"
    )
    return 0


if __name__ == "__main__":
    if not os.environ.get("CONFIG_REFUSALS_ROOT"):
        os.chdir(
            subprocess.run(
                ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
            ).stdout.strip()
        )
    sys.exit(main())

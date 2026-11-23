#!/usr/bin/env python3
"""Assert every router the server mounts goes through its mount ledger.

The transport refusal matrix (`tests/transport_refusal_matrix_e2e_pg.rs`) reads the
ledger of a real server and fails on a transport it has no row for. That forces a row
only for a mount the ledger records, so this gate makes recording the only way to
mount: in `crates/fraiseql-server/src/server/routing` and `.../server/capabilities`,
every `.merge(`, `.nest(` and `.nest_service(` call must be the ledger's own
(`mounts.merge(`, `mounts.nest(`, `ledger.merge(`, `ledger.nest(`). Any other is
reported at its file, line and column, each site on its own: a count would let a new
unrecorded mount pass beside a removed recorded one.

Test files (`*tests.rs`, anything under a `tests/` directory) are not scanned: they
build routers to test, not to serve. Comments and string contents are blanked before
matching.

A scan that finds no ledger call at all fails too: it read the wrong tree, and a gate
that matched nothing would report a clean tree it never saw.

`MOUNT_LEDGER_ROOT` sets the repository root instead (for tools/tests/mount_ledger_test.sh).
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

SCANNED = (
    "crates/fraiseql-server/src/server/routing",
    "crates/fraiseql-server/src/server/capabilities",
)
MOUNT_CALL = re.compile(r"\.\s*(merge|nest_service|nest)\s*\(")
LEDGER_RECEIVER = re.compile(r"\b(mounts|ledger)\s*$")


def strip_comments(text: str) -> str:
    """Blank out comments and the contents of string literals, keeping every offset."""
    out = []
    i, n = 0, len(text)
    in_string = False
    while i < n:
        c = text[i]
        if in_string:
            if c == "\\" and i + 1 < n:
                out.append("  " if text[i + 1] != "\n" else " \n")
                i += 2
                continue
            if c == '"':
                in_string = False
                out.append(c)
            else:
                out.append("\n" if c == "\n" else " ")
            i += 1
        elif c == "'" and text.startswith("\"'", i + 1):
            out.append("   ")
            i += 3
        elif c == '"':
            in_string = True
            out.append(c)
            i += 1
        elif text.startswith("//", i):
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


def is_test_file(path: Path) -> bool:
    return path.name.endswith("tests.rs") or "tests" in path.parts


def main() -> int:
    root = Path(os.environ.get("MOUNT_LEDGER_ROOT", Path(__file__).resolve().parent.parent))
    violations: list[str] = []
    ledger_calls = 0
    scanned = 0
    for directory in SCANNED:
        for path in sorted((root / directory).rglob("*.rs")):
            relative = path.relative_to(root)
            if is_test_file(relative):
                continue
            scanned += 1
            text = strip_comments(path.read_text(encoding="utf-8"))
            for match in MOUNT_CALL.finditer(text):
                if LEDGER_RECEIVER.search(text[: match.start()]):
                    ledger_calls += 1
                    continue
                line = text.count("\n", 0, match.start()) + 1
                column = match.start() - (text.rfind("\n", 0, match.start()) + 1) + 1
                violations.append(
                    f"{relative}:{line}:{column}: `.{match.group(1)}(` mounts a router "
                    "without recording it; use `self.mounts.merge(` / `.nest(`"
                )
    failed = False
    if violations:
        print("mount ledger: FAILED")
        for violation in violations:
            print(f"  {violation}")
        failed = True
    if scanned == 0 or ledger_calls == 0:
        print(
            f"mount ledger: FAILED: scanned {scanned} file(s) and found {ledger_calls} "
            "ledger call(s); the gate is not reading the server's mount sites."
        )
        failed = True
    if failed:
        return 1
    print(f"mount ledger: ok ({ledger_calls} mount site(s) recorded across {scanned} file(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

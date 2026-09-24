#!/usr/bin/env python3
"""Assert every database-gated test binary's document loads, in a run with no database.

A suite like the `*_e2e_pg.rs` ones self-skips when `DATABASE_URL` is unset, and it
skips *before* it compiles the document it serves. So a change that makes the schema
loader refuse that document — a new load-time check, a renamed section — is invisible in
every run without a database: the suite reports its tests as passed. `2b843cd27` is the
case that made this gate: a to-one uniqueness refusal rejected two suites' documents, 12
tests were red for a session, and `make preflight` (which runs no tests) and the
database-free test leg were both green over them.

The loading step itself needs no database, so each such suite carries one test that
does only that:

    fn the_document_loads_without_a_database()

This gate does two things:

1. Every `crates/*/tests/*.rs` that loads a document (it names one of `LOADERS`
   outside a comment) and consults a database (`DB_GATED`) must define that test,
   and the test must not itself consult `DATABASE_URL` — a guard that self-skips is
   the defect again. A binary that defines the test is in scope whether or not it
   still matches both patterns.
2. Unless `--static-only`, it runs exactly those tests and requires each suite to
   report its guard as `ok`. Counted, not trusted: a `--exact` filter that matches
   nothing exits 0.

`E2E_DOCUMENTS_ROOT` overrides the tree scanned (for the self-test in
tools/tests/e2e_documents_test.sh).
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

GUARD = "the_document_loads_without_a_database"

# What it looks like to load a document in these suites: compile one through the CLI,
# merge a shipped example, parse compiled JSON, or author compiled JSON by hand (every
# hand-written one carries the producer version stamp the loader requires).
LOADERS = re.compile(
    r"\b(compile_to_schema|merge_from_domains|CompiledSchema::from_json|CURRENT_FRAISEQL_VERSION)\b"
)

# What it looks like to find a database: the name is not the criterion — `pipeline_e2e_test`
# skips on its own env var before it compiles, and is exposed the same way.
DB_GATED = re.compile(r"\b(try_database_url|DATABASE_URL|fraiseql_test_support::postgres)\b")

SELF_SKIP = re.compile(r"\b(try_database_url|DATABASE_URL)\b")


def repo_root() -> Path:
    override = os.environ.get("E2E_DOCUMENTS_ROOT")
    if override:
        return Path(override)
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True
        ).stdout.strip()
    )


def strip_comments(src: str) -> str:
    """Drop `//` and `/* */` comments; string literals are left alone, which errs toward
    counting a loader named inside one — the safe direction for this gate."""
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def guard_body(src: str) -> str | None:
    """The body of the guard fn, or None if it is not defined."""
    m = re.search(rf"\bfn\s+{GUARD}\s*\(\s*\)[^{{]*\{{", src)
    if not m:
        return None
    depth, i = 1, m.end()
    while i < len(src) and depth:
        depth += {"{": 1, "}": -1}.get(src[i], 0)
        i += 1
    return src[m.end() : i - 1]


def scan(root: Path) -> tuple[dict[str, list[str]], list[str]]:
    """Return ({crate: [suite stems]}, [problems])."""
    suites: dict[str, list[str]] = {}
    problems: list[str] = []
    for path in sorted(root.glob("crates/*/tests/*.rs")):
        src = strip_comments(path.read_text())
        body = guard_body(src)
        # A binary carrying the guard is always run, whatever the heuristic says: a
        # document edited so that it no longer names a loader must not thereby leave
        # the gate's scope with its guard unrun.
        if body is None and not (LOADERS.search(src) and DB_GATED.search(src)):
            continue
        rel = path.relative_to(root)
        if body is None:
            problems.append(f"{rel}: loads a document but defines no `fn {GUARD}()`")
            continue
        if SELF_SKIP.search(body):
            problems.append(f"{rel}: `{GUARD}` consults DATABASE_URL, so it can skip")
            continue
        suites.setdefault(path.parent.parent.name, []).append(path.stem)
    return suites, problems


def run(suites: dict[str, list[str]]) -> list[str]:
    problems: list[str] = []
    env = {k: v for k, v in os.environ.items() if k != "DATABASE_URL"}
    for crate, stems in sorted(suites.items()):
        # --no-fail-fast: otherwise cargo stops at the first red binary and the count
        # below reports the unrun ones as failures too.
        cmd = ["cargo", "test", "-p", crate, "--all-features", "--no-fail-fast"]
        for stem in stems:
            cmd += ["--test", stem]
        cmd += ["--", "--exact", GUARD]
        print("  $ " + " ".join(cmd), flush=True)
        proc = subprocess.run(cmd, env=env, capture_output=True, text=True)
        out = proc.stdout + proc.stderr
        passed = len(re.findall(rf"^test {GUARD} \.\.\. ok$", out, flags=re.M))
        if proc.returncode != 0 or passed != len(stems):
            problems.append(
                f"{crate}: {passed} of {len(stems)} guards passed (cargo rc={proc.returncode})"
            )
            print(out[-6000:])
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--static-only", action="store_true", help="scan, do not run")
    args = parser.parse_args()

    suites, problems = scan(repo_root())
    total = sum(len(s) for s in suites.values())
    if not problems and not args.static_only:
        problems = run(suites)

    if problems:
        print("e2e documents: FAILED")
        for p in problems:
            print(f"  {p}")
        return 1
    verb = "carry" if args.static_only else "pass"
    print(f"e2e documents: all {total} database-gated document-loading test binaries {verb} `{GUARD}`")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""No two SQL fixtures may declare the same `public` relation.

`tests/sql/postgres/init.sql` (+ `init-analytics.sql`) is the ONE owner of the
`public` integration fixtures. `crates/fraiseql-db/tests/seed_fixture_integrity.rs`
says so in its own docstring, and everything it asserts rests on that premise:
the Dagger `pgService` and `docker/docker-compose.test.yml` mount those paths,
and a suite needing writable relations is expected to create its own uniquely
named ones.

`docker/e2e/init-postgres.sql` was a second owner of one of those names (#1281).
It declared `public.tb_user` as `id SERIAL, name TEXT` against the seed's
`id UUID, data JSONB`, both under `CREATE TABLE IF NOT EXISTS`, so whichever
loaded second was a silent no-op and the loser's dependent objects then failed to
build. Loading the e2e fixture and then the seed into one database left `v_users`
present and `v_user`, `v_post` and `v_order` missing, with the seed exiting 3 —
which is the state #1229 reported and could not attribute, because its grep
covered `crates/`, `tests/` and `tools/` and that file is under `docker/`.

What is checked: every `CREATE TABLE` / `CREATE VIEW` / `CREATE MATERIALIZED
VIEW` in the fixture trees below, unqualified or `public.`-qualified. A relation
declared by more than one FILE is a finding. Redeclaring within one file is not —
that is the file's own business, and `CREATE OR REPLACE VIEW` legitimately does it.

Relations qualified with another schema are skipped: a fixture that gives itself
a schema is exactly the fix this gate exists to encourage.

⚠ Deliberately narrow. A repository-wide "no two files declare the same relation"
rule has false positives everywhere — benchmark fixtures, example migrations and
SDK docs all declare a `v_users`, and legitimately, because they never share a
database. The invariant here is about the trees whose files are loaded into the
SAME database, and widening it means naming another such tree, not dropping the
constraint.

Overrides, for testing:
  FIXTURE_COLLISION_ROOT=<dir>   tree to check instead of the repo root
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

# Trees whose .sql files can end up in one database. Adding a tree here is how
# this gate grows; the paths are relative to the repo root.
FIXTURE_TREES = ("tests/sql", "docker")

CREATE_RELATION = re.compile(
    r"CREATE\s+(?:OR\s+REPLACE\s+)?(?:TABLE|(?:MATERIALIZED\s+)?VIEW)\s+"
    r"(?:IF\s+NOT\s+EXISTS\s+)?"
    r'("?[A-Za-z_][\w$]*"?(?:\s*\.\s*"?[A-Za-z_][\w$]*"?)?)',
    re.IGNORECASE,
)


def repo_root() -> Path:
    env = os.environ.get("FIXTURE_COLLISION_ROOT")
    if env:
        return Path(env)
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    )


def public_relation(raw: str) -> str | None:
    """The `public` relation this CREATE names, or None if it is elsewhere."""
    parts = [p.strip().strip('"').lower() for p in raw.split(".")]
    if len(parts) == 2:
        return parts[1] if parts[0] == "public" else None
    return parts[0]


def main() -> int:
    root = repo_root()
    files = sorted(
        f
        for tree in FIXTURE_TREES
        if (root / tree).is_dir()
        for f in (root / tree).rglob("*.sql")
    )
    if not files:
        print(
            "fixture-relation-collisions: FAIL — scanned zero .sql files under "
            f"{', '.join(FIXTURE_TREES)}; the layout changed and this gate went blind",
            file=sys.stderr,
        )
        return 1

    # file → relations, so a redeclaration WITHIN one file is not a finding.
    owners: dict[str, set[str]] = {}
    declarations = 0
    for f in files:
        rel = f.relative_to(root).as_posix()
        for m in CREATE_RELATION.finditer(f.read_text(encoding="utf-8")):
            name = public_relation(m.group(1))
            if name is None:
                continue
            declarations += 1
            owners.setdefault(name, set()).add(rel)

    collisions = {n: fs for n, fs in owners.items() if len(fs) > 1}
    if collisions:
        print("fixture-relation-collisions: FAIL", file=sys.stderr)
        print(
            "\nTwo fixtures declare the same `public` relation. Both can be loaded into\n"
            "one database, and under `CREATE ... IF NOT EXISTS` the second is a silent\n"
            "no-op whose dependent objects then fail to build (#1281):\n",
            file=sys.stderr,
        )
        for name in sorted(collisions):
            print(f"  public.{name}", file=sys.stderr)
            for f in sorted(collisions[name]):
                print(f"      {f}", file=sys.stderr)
        print(
            "\n  Give the newcomer names that cannot collide — its own schema, or a\n"
            "  prefix of its own. tests/sql/postgres/init.sql is the declared owner of\n"
            "  the `public` integration fixtures; see seed_fixture_integrity.rs.\n",
            file=sys.stderr,
        )
        return 1

    print(
        f"fixture-relation-collisions: OK — {len(files)} fixture file(s), "
        f"{declarations} relation declaration(s), {len(owners)} distinct `public` "
        "relation(s), each owned by one file."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
